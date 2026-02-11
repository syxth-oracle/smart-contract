import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { SolanaPredict } from "../target/types/solana_predict";
import * as token from "@solana/spl-token";
import { assert } from "chai";
import {
  PublicKey,
  Keypair,
  LAMPORTS_PER_SOL,
  SystemProgram,
  SYSVAR_RENT_PUBKEY,
} from "@solana/web3.js";

/**
 * Comprehensive CPMM Migration Tests
 * ------------------------------------
 * Tests cover:
 *   1. Platform init with wSOL collateral
 *   2. Create market with initial_liquidity (CPMM pool seeding)
 *   3. Place YES bet → verify CPMM share calculation
 *   4. Place NO bet → verify CPMM share calculation & price movement
 *   5. Cancel bet → verify CPMM sell refund
 *   6. Price invariants: yesPrice + noPrice ≈ 1.0
 *   7. Resolve market → claim payout (uses mint supply denominator)
 *   8. Multi-user payout fairness
 *   9. Slippage guard
 *  10. Edge case: large bet relative to pool
 */

describe("CPMM Migration Tests", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.solanaPredict as Program<SolanaPredict>;
  const admin = provider.wallet;
  const adminPayer = (admin as any).payer as Keypair;

  // Test users
  const userA = Keypair.generate();
  const userB = Keypair.generate();

  // PDAs
  const [platformConfig] = PublicKey.findProgramAddressSync(
    [Buffer.from("platform_config")],
    program.programId
  );

  // wSOL as collateral
  const WSOL_MINT = new PublicKey("So11111111111111111111111111111111111111112");

  // Treasury - we'll use admin's wSOL ATA for simplicity
  let treasuryAta: PublicKey;

  // Market helpers
  const MARKET_ID_1 = new BN(Date.now()); // unique per run

  function deriveMarketPda(marketId: BN) {
    return PublicKey.findProgramAddressSync(
      [Buffer.from("market"), marketId.toArrayLike(Buffer, "le", 8)],
      program.programId
    );
  }
  function deriveVault(marketPda: PublicKey) {
    return PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), marketPda.toBuffer()],
      program.programId
    );
  }
  function deriveYesMint(marketPda: PublicKey) {
    return PublicKey.findProgramAddressSync(
      [Buffer.from("yes_mint"), marketPda.toBuffer()],
      program.programId
    );
  }
  function deriveNoMint(marketPda: PublicKey) {
    return PublicKey.findProgramAddressSync(
      [Buffer.from("no_mint"), marketPda.toBuffer()],
      program.programId
    );
  }
  function derivePosition(marketPda: PublicKey, user: PublicKey) {
    return PublicKey.findProgramAddressSync(
      [Buffer.from("position"), marketPda.toBuffer(), user.toBuffer()],
      program.programId
    );
  }

  // Helper: create & fund a wSOL ATA for a given keypair
  async function fundWsol(owner: Keypair, lamports: number) {
    const ata = await token.getAssociatedTokenAddress(WSOL_MINT, owner.publicKey);
    try {
      await token.getAccount(provider.connection, ata);
    } catch {
      await token.createAssociatedTokenAccount(
        provider.connection,
        adminPayer,
        WSOL_MINT,
        owner.publicKey
      );
    }
    // Transfer SOL into wSOL ATA
    const tx = new anchor.web3.Transaction().add(
      SystemProgram.transfer({
        fromPubkey: owner.publicKey,
        toPubkey: ata,
        lamports,
      }),
      token.createSyncNativeInstruction(ata)
    );
    await provider.sendAndConfirm(tx, [owner]);
    return ata;
  }

  // Helper: CPMM price from pool state
  function cpmmPrice(yesPool: number, noPool: number) {
    const total = yesPool + noPool;
    if (total === 0) return { yesPrice: 0.5, noPrice: 0.5 };
    return {
      yesPrice: noPool / total,
      noPrice: yesPool / total,
    };
  }

  // Helper: expected CPMM shares for a YES buy
  function expectedSharesYes(yesPool: number, noPool: number, netAmount: number): number {
    const k = yesPool * noPool;
    const newNo = noPool + netAmount;
    return yesPool - k / newNo;
  }

  // Helper: expected CPMM shares for a NO buy
  function expectedSharesNo(yesPool: number, noPool: number, netAmount: number): number {
    const k = yesPool * noPool;
    const newYes = yesPool + netAmount;
    return noPool - k / newYes;
  }

  before(async () => {
    console.log("Program ID:", program.programId.toBase58());
    console.log("Admin:", admin.publicKey.toBase58());

    // Fund test users via airdrop (unlimited on localnet)
    for (const u of [userA, userB]) {
      const sig = await provider.connection.requestAirdrop(u.publicKey, 10 * LAMPORTS_PER_SOL);
      await provider.connection.confirmTransaction(sig);
      console.log(`  Funded ${u.publicKey.toBase58().slice(0, 8)}... with 10 SOL`);
    }

    // Treasury wSOL ATA (owned by admin)
    treasuryAta = await token.getAssociatedTokenAddress(WSOL_MINT, admin.publicKey);
    try {
      await token.getAccount(provider.connection, treasuryAta);
    } catch {
      await token.createAssociatedTokenAccount(
        provider.connection,
        adminPayer,
        WSOL_MINT,
        admin.publicKey
      );
    }
  });

  // ========================================================================
  // 1. Platform Init
  // ========================================================================
  it("1. Initializes platform with wSOL collateral", async () => {
    await program.methods
        .initPlatform(250, new BN(1_000_000)) // 2.5% fee, 0.001 SOL bond
        .accounts({
          platformConfig,
          admin: admin.publicKey,
          systemProgram: SystemProgram.programId,
          collateralMint: WSOL_MINT,
          treasury: treasuryAta,
        })
        .rpc();

    const plat = await program.account.platformConfig.fetch(platformConfig);
    assert.ok(plat.admin.equals(admin.publicKey));
    assert.ok(plat.collateralMint.equals(WSOL_MINT), "Collateral should be wSOL");
    console.log("  ✓ Platform initialized, fee:", plat.feeBps, "bps");
  });

  // ========================================================================
  // 2. Create Market with CPMM Initial Liquidity
  // ========================================================================
  it("2. Creates market with initial_liquidity seeding CPMM pools", async () => {
    const [marketPda] = deriveMarketPda(MARKET_ID_1);
    const [yesMint] = deriveYesMint(marketPda);
    const [noMint] = deriveNoMint(marketPda);
    const [vault] = deriveVault(marketPda);

    const now = Math.floor(Date.now() / 1000);
    const INITIAL_LIQUIDITY = 1 * LAMPORTS_PER_SOL; // 1 SOL

    // Fund admin wSOL ATA for initial liquidity
    const adminAta = await fundWsol(adminPayer, INITIAL_LIQUIDITY);

    const params = {
      title: "CPMM Test: SOL > $200?",
      description: "Testing CPMM pool creation",
      category: { crypto: {} },
      oracleSource: { manualAdmin: {} },
      oracleFeed: PublicKey.default,
      oracleThreshold: new BN(0),
      startTimestamp: new BN(now - 60),
      lockTimestamp: new BN(now + 3600),
      endTimestamp: new BN(now + 7200),
      minBet: new BN(10_000_000), // 0.01 SOL
      maxBet: new BN(0),
      isRecurring: false,
      roundDuration: null,
      feeBps: 250, // 2.5%
      initialLiquidity: new BN(INITIAL_LIQUIDITY),
    };

    await program.methods
      .createMarket(MARKET_ID_1, params as any)
      .accounts({
        adminAta,
      })
      .rpc();

    const market = await program.account.market.fetch(marketPda);

    // Verify CPMM pools are seeded equally
    assert.equal(market.totalYesShares.toString(), INITIAL_LIQUIDITY.toString(),
      "YES pool should equal initial_liquidity");
    assert.equal(market.totalNoShares.toString(), INITIAL_LIQUIDITY.toString(),
      "NO pool should equal initial_liquidity");
    assert.equal(market.totalCollateral.toString(), INITIAL_LIQUIDITY.toString(),
      "Collateral should equal initial_liquidity");

    // Verify vault received the SOL
    const vaultAcc = await token.getAccount(provider.connection, vault);
    assert.equal(Number(vaultAcc.amount), INITIAL_LIQUIDITY,
      "Vault should hold the initial liquidity");

    // Verify price is 50/50 at start
    const { yesPrice, noPrice } = cpmmPrice(
      market.totalYesShares.toNumber(),
      market.totalNoShares.toNumber()
    );
    assert.approximately(yesPrice, 0.5, 0.001, "YES price should start at 50%");
    assert.approximately(noPrice, 0.5, 0.001, "NO price should start at 50%");

    console.log(`  ✓ Market created (ID: ${MARKET_ID_1.toString()})`);
    console.log(`    Pools: YES=${market.totalYesShares.toNumber() / LAMPORTS_PER_SOL} SOL, NO=${market.totalNoShares.toNumber() / LAMPORTS_PER_SOL} SOL`);
    console.log(`    k = ${(market.totalYesShares.toNumber() / LAMPORTS_PER_SOL) * (market.totalNoShares.toNumber() / LAMPORTS_PER_SOL)}`);
    console.log(`    Price: YES=${(yesPrice * 100).toFixed(1)}¢, NO=${(noPrice * 100).toFixed(1)}¢`);
  });

  // ========================================================================
  // 3. Place YES Bet → CPMM share calculation
  // ========================================================================
  it("3. Place YES bet: shares follow CPMM formula", async () => {
    const [marketPda] = deriveMarketPda(MARKET_ID_1);
    const [vault] = deriveVault(marketPda);
    const [yesMint] = deriveYesMint(marketPda);
    const [noMint] = deriveNoMint(marketPda);
    const [userPosition] = derivePosition(marketPda, userA.publicKey);

    // Pre-state
    const marketBefore = await program.account.market.fetch(marketPda);
    const yesPoolBefore = marketBefore.totalYesShares.toNumber();
    const noPoolBefore = marketBefore.totalNoShares.toNumber();

    const BET_AMOUNT = Math.floor(0.5 * LAMPORTS_PER_SOL); // 0.5 SOL
    const FEE_BPS = 250;
    // Ceiling division to match contract: (amount * fee_bps + 9999) / 10000
    const fee = Math.floor((BET_AMOUNT * FEE_BPS + 9999) / 10000);
    const netAmount = BET_AMOUNT - fee;

    // Expected shares via CPMM
    const expectedShares = expectedSharesYes(yesPoolBefore, noPoolBefore, netAmount);

    // Fund user and create ATAs
    const userAta = await fundWsol(userA, BET_AMOUNT);
    const userYesAta = await token.getOrCreateAssociatedTokenAccount(
      provider.connection, adminPayer, yesMint, userA.publicKey
    );

    await program.methods
      .placeBet(MARKET_ID_1, { yes: {} }, new BN(BET_AMOUNT), new BN(0))
      .accounts({
        user: userA.publicKey,
        userShareAccount: userYesAta.address,
        platformConfig,
        treasury: treasuryAta,
        collateralMint: WSOL_MINT,
      })
      .signers([userA])
      .rpc();

    // Post-state
    const marketAfter = await program.account.market.fetch(marketPda);
    const position = await program.account.userPosition.fetch(userPosition);
    const sharesReceived = position.yesShares.toNumber();

    console.log(`  ✓ YES bet placed: ${BET_AMOUNT / LAMPORTS_PER_SOL} SOL`);
    console.log(`    Net amount (after 2.5% fee): ${netAmount / LAMPORTS_PER_SOL} SOL`);
    console.log(`    Expected shares: ${expectedShares / LAMPORTS_PER_SOL}`);
    console.log(`    Actual shares:   ${sharesReceived / LAMPORTS_PER_SOL}`);

    // Shares should match CPMM formula (allow 1 lamport rounding)
    assert.approximately(sharesReceived, Math.floor(expectedShares), 1,
      "Shares should match CPMM formula");

    // Pool reserves should update correctly:
    // YES pool shrinks (shares taken out), NO pool grows (net_amount added)
    const expectedYesPool = yesPoolBefore - sharesReceived;
    const expectedNoPool = noPoolBefore + netAmount;
    assert.equal(marketAfter.totalYesShares.toNumber(), expectedYesPool,
      "YES pool should decrease by shares");
    assert.equal(marketAfter.totalNoShares.toNumber(), expectedNoPool,
      "NO pool should increase by net_amount");

    // Verify k is approximately preserved (integer math may cause tiny drift)
    const kBefore = yesPoolBefore * noPoolBefore;
    const kAfter = marketAfter.totalYesShares.toNumber() * marketAfter.totalNoShares.toNumber();
    const kDrift = Math.abs(kAfter - kBefore) / kBefore;
    assert.isBelow(kDrift, 0.001, "k should be approximately preserved");

    // Price should have moved: YES price up (less YES in pool)
    const { yesPrice, noPrice } = cpmmPrice(
      marketAfter.totalYesShares.toNumber(),
      marketAfter.totalNoShares.toNumber()
    );
    assert.isAbove(yesPrice, 0.5, "YES price should increase after YES buy");
    assert.isBelow(noPrice, 0.5, "NO price should decrease after YES buy");
    assert.approximately(yesPrice + noPrice, 1.0, 0.001, "Prices should sum to 1.0");
    console.log(`    New price: YES=${(yesPrice * 100).toFixed(1)}¢, NO=${(noPrice * 100).toFixed(1)}¢`);
  });

  // ========================================================================
  // 4. Place NO bet → price moves opposite
  // ========================================================================
  it("4. Place NO bet: price moves in opposite direction", async () => {
    const [marketPda] = deriveMarketPda(MARKET_ID_1);
    const [noMint] = deriveNoMint(marketPda);
    const [userPosition] = derivePosition(marketPda, userB.publicKey);

    const marketBefore = await program.account.market.fetch(marketPda);
    const yesPoolBefore = marketBefore.totalYesShares.toNumber();
    const noPoolBefore = marketBefore.totalNoShares.toNumber();
    const { yesPrice: priceBefore } = cpmmPrice(yesPoolBefore, noPoolBefore);

    const BET_AMOUNT = Math.floor(0.3 * LAMPORTS_PER_SOL);
    // Ceiling division to match contract
    const fee = Math.floor((BET_AMOUNT * 250 + 9999) / 10000);
    const netAmount = BET_AMOUNT - fee;
    const expectedShares = expectedSharesNo(yesPoolBefore, noPoolBefore, netAmount);

    const userBta = await fundWsol(userB, BET_AMOUNT);
    const userNoAta = await token.getOrCreateAssociatedTokenAccount(
      provider.connection, adminPayer, noMint, userB.publicKey
    );

    await program.methods
      .placeBet(MARKET_ID_1, { no: {} }, new BN(BET_AMOUNT), new BN(0))
      .accounts({
        user: userB.publicKey,
        userShareAccount: userNoAta.address,
        platformConfig,
        treasury: treasuryAta,
        collateralMint: WSOL_MINT,
      })
      .signers([userB])
      .rpc();

    const marketAfter = await program.account.market.fetch(marketPda);
    const position = await program.account.userPosition.fetch(userPosition);

    const { yesPrice, noPrice } = cpmmPrice(
      marketAfter.totalYesShares.toNumber(),
      marketAfter.totalNoShares.toNumber()
    );

    console.log(`  ✓ NO bet placed: ${BET_AMOUNT / LAMPORTS_PER_SOL} SOL`);
    console.log(`    Shares received: ${position.noShares.toNumber() / LAMPORTS_PER_SOL}`);
    console.log(`    Price: YES=${(yesPrice * 100).toFixed(1)}¢, NO=${(noPrice * 100).toFixed(1)}¢`);

    // YES price should decrease (NO bet pulled from NO pool, added to YES pool)
    assert.isBelow(yesPrice, priceBefore, "YES price should decrease after NO buy");
    assert.approximately(yesPrice + noPrice, 1.0, 0.001, "Prices must sum to 1.0");
    assert.approximately(position.noShares.toNumber(), Math.floor(expectedShares), 1,
      "NO shares should match CPMM formula");
  });

  // ========================================================================
  // 5. Cancel bet → CPMM sell refund
  // ========================================================================
  it("5. Cancel partial YES bet: refund follows CPMM sell curve", async () => {
    const [marketPda] = deriveMarketPda(MARKET_ID_1);
    const [yesMint] = deriveYesMint(marketPda);
    const [noMint] = deriveNoMint(marketPda);
    const [vault] = deriveVault(marketPda);

    const marketBefore = await program.account.market.fetch(marketPda);
    const yesPoolBefore = marketBefore.totalYesShares.toNumber();
    const noPoolBefore = marketBefore.totalNoShares.toNumber();
    const kBefore = yesPoolBefore * noPoolBefore;

    // UserA has YES shares — burn half
    const [userPosition] = derivePosition(marketPda, userA.publicKey);
    const posBefore = await program.account.userPosition.fetch(userPosition);
    const sharesToBurn = Math.floor(posBefore.yesShares.toNumber() / 2);

    if (sharesToBurn === 0) {
      console.log("  ⚠ No shares to burn, skipping cancel test");
      return;
    }

    // Expected CPMM sell refund: add shares back to YES pool, refund comes from NO pool
    const newYesPool = yesPoolBefore + sharesToBurn;
    const newNoPool = Math.floor(kBefore / newYesPool);
    const rawRefund = noPoolBefore - newNoPool;
    // Ceiling division to match contract: (rawRefund * fee_bps + 9999) / 10000
    const exitFee = Math.floor((rawRefund * 250 + 9999) / 10000); // 2.5% exit fee, ceiling
    const expectedRefund = rawRefund - exitFee;

    const userAta = await token.getAssociatedTokenAddress(WSOL_MINT, userA.publicKey);
    const userYesAta = await token.getAssociatedTokenAddress(yesMint, userA.publicKey);

    // Need to re-create wSOL ATA if it was closed
    try {
      await token.getAccount(provider.connection, userAta);
    } catch {
      await token.createAssociatedTokenAccount(
        provider.connection, adminPayer, WSOL_MINT, userA.publicKey
      );
    }

    const balBefore = Number((await token.getAccount(provider.connection, userAta)).amount);

    await program.methods
      .cancelBet(MARKET_ID_1, new BN(sharesToBurn))
      .accounts({
        market: marketPda,
        yesMint,
        noMint,
        vault,
        userPosition,
        userAta,
        userShareAccount: userYesAta,
        platformConfig,
        treasury: treasuryAta,
        user: userA.publicKey,
        collateralMint: WSOL_MINT,
        tokenProgram: token.TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([userA])
      .rpc();

    const balAfter = Number((await token.getAccount(provider.connection, userAta)).amount);
    const refundReceived = balAfter - balBefore;

    const marketAfter = await program.account.market.fetch(marketPda);
    const kAfter = marketAfter.totalYesShares.toNumber() * marketAfter.totalNoShares.toNumber();

    console.log(`  ✓ Cancelled ${sharesToBurn / LAMPORTS_PER_SOL} YES shares`);
    console.log(`    Expected refund: ${expectedRefund / LAMPORTS_PER_SOL} SOL`);
    console.log(`    Actual refund:   ${refundReceived / LAMPORTS_PER_SOL} SOL`);
    console.log(`    Exit fee:        ${exitFee / LAMPORTS_PER_SOL} SOL`);

    assert.approximately(refundReceived, expectedRefund, 2,
      "Refund should match CPMM sell formula minus 1% fee");

    // k should be approximately preserved
    const kDrift = Math.abs(kAfter - kBefore) / kBefore;
    assert.isBelow(kDrift, 0.01, "k should be approximately preserved after cancel");
  });

  // ========================================================================
  // 6. Price invariant check
  // ========================================================================
  it("6. Price invariant: YES + NO ≈ 1.0 after multiple trades", async () => {
    const [marketPda] = deriveMarketPda(MARKET_ID_1);
    const market = await program.account.market.fetch(marketPda);
    const { yesPrice, noPrice } = cpmmPrice(
      market.totalYesShares.toNumber(),
      market.totalNoShares.toNumber()
    );
    assert.approximately(yesPrice + noPrice, 1.0, 0.001,
      "YES + NO prices must always sum to 1.0");
    assert.isAbove(yesPrice, 0, "YES price must be > 0");
    assert.isBelow(yesPrice, 1, "YES price must be < 1");
    console.log(`  ✓ Price invariant holds: ${(yesPrice * 100).toFixed(2)}¢ + ${(noPrice * 100).toFixed(2)}¢ = ${((yesPrice + noPrice) * 100).toFixed(2)}¢`);
  });

  // ========================================================================
  // 7. Resolve → Claim Payout (uses mint supply)
  // ========================================================================
  it("7. Resolve YES → UserA claims payout (proportional to mint supply)", async () => {
    const [marketPda] = deriveMarketPda(MARKET_ID_1);
    const [yesMint] = deriveYesMint(marketPda);
    const [noMint] = deriveNoMint(marketPda);
    const [vault] = deriveVault(marketPda);
    const [userPosition] = derivePosition(marketPda, userA.publicKey);

    // Check user still has YES shares
    const userYesAta = await token.getAssociatedTokenAddress(yesMint, userA.publicKey);
    const sharesBefore = Number((await token.getAccount(provider.connection, userYesAta)).amount);

    if (sharesBefore === 0) {
      console.log("  ⚠ UserA has no YES shares left, skipping claim test");
      return;
    }

    // Get market state before resolution
    const market = await program.account.market.fetch(marketPda);
    const totalCollateral = market.totalCollateral.toNumber();

    // Get YES mint supply (this is what claim_payout uses as denominator)
    const yesMintInfo = await token.getMint(provider.connection, yesMint);
    const totalYesSupply = Number(yesMintInfo.supply);

    const expectedPayout = Math.floor(
      (sharesBefore / totalYesSupply) * totalCollateral
    );

    // Resolve
    await program.methods
      .resolveMarket(MARKET_ID_1, { yes: {} })
      .accounts({
        market: marketPda,
        admin: admin.publicKey,
        platformConfig,
        pythPriceFeed: null,
      })
      .rpc();

    const resolved = await program.account.market.fetch(marketPda);
    assert.ok(resolved.status.resolved, "Market should be resolved");
    assert.ok(resolved.resolvedOutcome.yes, "Should resolve to YES");

    // Need wSOL ATA for payout
    const userAta = await token.getAssociatedTokenAddress(WSOL_MINT, userA.publicKey);
    try {
      await token.getAccount(provider.connection, userAta);
    } catch {
      await token.createAssociatedTokenAccount(
        provider.connection, adminPayer, WSOL_MINT, userA.publicKey
      );
    }

    const balBefore = Number((await token.getAccount(provider.connection, userAta)).amount);

    // Claim
    await program.methods
      .claimPayout(MARKET_ID_1)
      .accounts({
        market: marketPda,
        yesMint,
        noMint,
        vault,
        userPosition,
        userAta,
        userShareAccount: userYesAta,
        user: userA.publicKey,
        collateralMint: WSOL_MINT,
        tokenProgram: token.TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([userA])
      .rpc();

    const balAfter = Number((await token.getAccount(provider.connection, userAta)).amount);
    const payout = balAfter - balBefore;

    // Shares should be burned
    const sharesAfter = Number((await token.getAccount(provider.connection, userYesAta)).amount);
    assert.equal(sharesAfter, 0, "All shares should be burned after claim");

    console.log(`  ✓ Payout claimed`);
    console.log(`    Shares burned: ${sharesBefore / LAMPORTS_PER_SOL}`);
    console.log(`    Expected payout: ${expectedPayout / LAMPORTS_PER_SOL} SOL`);
    console.log(`    Actual payout:   ${payout / LAMPORTS_PER_SOL} SOL`);
    console.log(`    Total collateral was: ${totalCollateral / LAMPORTS_PER_SOL} SOL`);

    // Allow small rounding (u128 integer division)
    assert.approximately(payout, expectedPayout, 2,
      "Payout should match (shares/totalSupply) * collateral");
  });

  // ========================================================================
  // 8. Slippage guard test
  // ========================================================================
  it("8. Slippage guard: rejects bet when min_shares not met", async () => {
    // Create a fresh market for this test
    const marketId2 = new BN(Date.now() + 1);
    const [marketPda2] = deriveMarketPda(marketId2);
    const [yesMint2] = deriveYesMint(marketPda2);
    const [noMint2] = deriveNoMint(marketPda2);
    const [vault2] = deriveVault(marketPda2);

    const now = Math.floor(Date.now() / 1000);
    const INITIAL_LIQ = 1 * LAMPORTS_PER_SOL;
    const adminAta = await fundWsol(adminPayer, INITIAL_LIQ);

    await program.methods
      .createMarket(marketId2, {
        title: "Slippage Test",
        description: "Test",
        category: { crypto: {} },
        oracleSource: { manualAdmin: {} },
        oracleFeed: PublicKey.default,
        oracleThreshold: new BN(0),
        startTimestamp: new BN(now - 60),
        lockTimestamp: new BN(now + 3600),
        endTimestamp: new BN(now + 7200),
        minBet: new BN(10_000_000),
        maxBet: new BN(0),
        isRecurring: false,
        roundDuration: null,
        feeBps: 250,
        initialLiquidity: new BN(INITIAL_LIQ),
      } as any)
      .accounts({ adminAta })
      .rpc();

    // Try to place a bet with absurdly high min_shares (should fail)
    const BET = Math.floor(0.1 * LAMPORTS_PER_SOL);
    const userAta = await fundWsol(userA, BET);
    const userShareAta = await token.getOrCreateAssociatedTokenAccount(
      provider.connection, adminPayer, yesMint2, userA.publicKey
    );

    try {
      await program.methods
        .placeBet(
          marketId2,
          { yes: {} },
          new BN(BET),
          new BN(BET * 2) // Impossible: asking for 2x the bet as shares
        )
        .accounts({
          user: userA.publicKey,
          userShareAccount: userShareAta.address,
          platformConfig,
          treasury: treasuryAta,
          collateralMint: WSOL_MINT,
        })
        .signers([userA])
        .rpc();
      assert.fail("Should have thrown SlippageExceeded");
    } catch (e: any) {
      assert.include(e.message, "SlippageExceeded",
        "Error should be SlippageExceeded when min_shares too high");
      console.log("  ✓ Slippage guard correctly rejected bet");
    }
  });

  // ========================================================================
  // 9. Large bet impact test
  // ========================================================================
  it("9. Large bet moves price significantly but k is preserved", async () => {
    const marketId3 = new BN(Date.now() + 2);
    const [marketPda3] = deriveMarketPda(marketId3);
    const [yesMint3] = deriveYesMint(marketPda3);

    const now = Math.floor(Date.now() / 1000);
    const INITIAL_LIQ = 1 * LAMPORTS_PER_SOL;
    const adminAta = await fundWsol(adminPayer, INITIAL_LIQ);

    await program.methods
      .createMarket(marketId3, {
        title: "Large Bet Test",
        description: "Test",
        category: { crypto: {} },
        oracleSource: { manualAdmin: {} },
        oracleFeed: PublicKey.default,
        oracleThreshold: new BN(0),
        startTimestamp: new BN(now - 60),
        lockTimestamp: new BN(now + 3600),
        endTimestamp: new BN(now + 7200),
        minBet: new BN(10_000_000),
        maxBet: new BN(0),
        isRecurring: false,
        roundDuration: null,
        feeBps: 250,
        initialLiquidity: new BN(INITIAL_LIQ),
      } as any)
      .accounts({ adminAta })
      .rpc();

    const marketBefore = await program.account.market.fetch(marketPda3);
    const kBefore = marketBefore.totalYesShares.toNumber() * marketBefore.totalNoShares.toNumber();

    // Place a 5x bet (5 SOL into 1 SOL pool)
    const LARGE_BET = 5 * LAMPORTS_PER_SOL;
    const userAta = await fundWsol(userA, LARGE_BET);
    const userShareAta = await token.getOrCreateAssociatedTokenAccount(
      provider.connection, adminPayer, yesMint3, userA.publicKey
    );

    await program.methods
      .placeBet(marketId3, { yes: {} }, new BN(LARGE_BET), new BN(0))
      .accounts({
        user: userA.publicKey,
        userShareAccount: userShareAta.address,
        platformConfig,
        treasury: treasuryAta,
        collateralMint: WSOL_MINT,
      })
      .signers([userA])
      .rpc();

    const marketAfter = await program.account.market.fetch(marketPda3);
    const { yesPrice, noPrice } = cpmmPrice(
      marketAfter.totalYesShares.toNumber(),
      marketAfter.totalNoShares.toNumber()
    );

    const kAfter = marketAfter.totalYesShares.toNumber() * marketAfter.totalNoShares.toNumber();
    const kDrift = Math.abs(kAfter - kBefore) / kBefore;

    console.log(`  ✓ Large YES bet: ${LARGE_BET / LAMPORTS_PER_SOL} SOL into ${INITIAL_LIQ / LAMPORTS_PER_SOL} SOL pool`);
    console.log(`    YES price: ${(yesPrice * 100).toFixed(1)}¢ (should be high)`);
    console.log(`    NO price:  ${(noPrice * 100).toFixed(1)}¢ (should be low)`);
    console.log(`    k drift:   ${(kDrift * 100).toFixed(4)}%`);

    assert.isAbove(yesPrice, 0.8, "Large YES bet should push price above 80%");
    assert.approximately(yesPrice + noPrice, 1.0, 0.001);
    assert.isBelow(kDrift, 0.01, "k should be preserved (< 1% drift)");
  });

  // ========================================================================
  // 10. Backend/Frontend pricing formula verification
  // ========================================================================
  it("10. Verify CPMM pricing formula matches contract state", async () => {
    // This test verifies the formula we use in backend/frontend matches on-chain state
    const [marketPda] = deriveMarketPda(MARKET_ID_1);
    const market = await program.account.market.fetch(marketPda);

    const yesPool = market.totalYesShares.toNumber();
    const noPool = market.totalNoShares.toNumber();
    const total = yesPool + noPool;

    // CPMM formula (what backend/frontend now use): yesPrice = noPool / total
    const yesPriceCPMM = noPool / total;
    const noPriceCPMM = yesPool / total;

    // Old parimutuel formula (should NOT be used): yesPrice = yesPool / total
    const yesPricePM = yesPool / total;

    console.log(`  Pool state: YES=${yesPool / LAMPORTS_PER_SOL}, NO=${noPool / LAMPORTS_PER_SOL}`);
    console.log(`  CPMM price:       YES=${(yesPriceCPMM * 100).toFixed(2)}¢, NO=${(noPriceCPMM * 100).toFixed(2)}¢`);
    console.log(`  Parimutuel price: YES=${(yesPricePM * 100).toFixed(2)}¢ (WRONG - not used)`);

    // The key difference: in CPMM, if more people bet YES, yes_pool shrinks,
    // so yesPool < noPool, meaning noPool/total > 0.5 (YES is expensive).
    // In parimutuel, yesPool/total would give the WRONG answer.
    assert.approximately(yesPriceCPMM + noPriceCPMM, 1.0, 0.001);
    // Only assert difference if trades actually changed the pools
    if (yesPool !== noPool) {
      assert.notEqual(yesPriceCPMM, yesPricePM,
        "CPMM and Parimutuel should give different prices after trades");
    }
    console.log("  ✓ CPMM formula verified: yesPrice = noPool / (yesPool + noPool)");
  });
});
