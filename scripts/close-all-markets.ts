/**
 * Script to close all existing market accounts on-chain.
 * This reclaims rent SOL and removes stale data from the program.
 * 
 * Usage: npx ts-mocha --timeout 120000 -p ./tsconfig.json scripts/close-all-markets.ts
 * Or:    npx tsx scripts/close-all-markets.ts
 */

import { Program, AnchorProvider, Wallet, BN } from "@coral-xyz/anchor";
import { Connection, Keypair, PublicKey } from "@solana/web3.js";
import { TOKEN_PROGRAM_ID } from "@solana/spl-token";
import * as fs from "fs";
import * as path from "path";

// Load IDL
const idlPath = path.join(__dirname, "../target/idl/solana_predict.json");
const idl = JSON.parse(fs.readFileSync(idlPath, "utf-8"));

// Load wallet keypair
const keypairPath = path.join(__dirname, "../id.json");
const keypairData = JSON.parse(fs.readFileSync(keypairPath, "utf-8"));
const adminKeypair = Keypair.fromSecretKey(Uint8Array.from(keypairData));

const RPC_URL = "https://api.devnet.solana.com";

async function main() {
    const connection = new Connection(RPC_URL, "confirmed");
    const wallet = new Wallet(adminKeypair);
    const provider = new AnchorProvider(connection, wallet, {
        preflightCommitment: "confirmed",
    });

    const program = new Program(idl, provider);

    console.log("Admin:", adminKeypair.publicKey.toBase58());
    console.log("Program:", program.programId.toBase58());

    // Fetch all market accounts
    const markets = await (program.account as any).market.all();
    console.log(`Found ${markets.length} market(s) on-chain\n`);

    if (markets.length === 0) {
        console.log("No markets to close.");
        return;
    }

    const balanceBefore = await connection.getBalance(adminKeypair.publicKey);
    console.log(`Balance before: ${balanceBefore / 1e9} SOL\n`);

    for (const { publicKey, account } of markets) {
        const data = account as any;
        const marketId = data.marketId as BN;
        const mid = new BN(marketId);

        console.log(`Closing market #${marketId.toString()} "${data.title}" (${publicKey.toBase58()})`);

        // Derive associated PDAs
        const [marketPda] = PublicKey.findProgramAddressSync(
            [Buffer.from("market"), mid.toArrayLike(Buffer, "le", 8)],
            program.programId
        );
        const [yesMint] = PublicKey.findProgramAddressSync(
            [Buffer.from("yes_mint"), marketPda.toBuffer()],
            program.programId
        );
        const [noMint] = PublicKey.findProgramAddressSync(
            [Buffer.from("no_mint"), marketPda.toBuffer()],
            program.programId
        );
        const [vault] = PublicKey.findProgramAddressSync(
            [Buffer.from("vault"), marketPda.toBuffer()],
            program.programId
        );
        const [platformConfig] = PublicKey.findProgramAddressSync(
            [Buffer.from("platform_config")],
            program.programId
        );

        try {
            const tx = await program.methods
                .closeMarket(mid)
                .accounts({
                    market: marketPda,
                    yesMint,
                    noMint,
                    vault,
                    admin: adminKeypair.publicKey,
                    tokenProgram: TOKEN_PROGRAM_ID,
                })
                .rpc();

            console.log(`  ✓ Closed. Tx: ${tx}`);
        } catch (err: any) {
            console.error(`  ✗ Failed: ${err.message}`);
        }
    }

    const balanceAfter = await connection.getBalance(adminKeypair.publicKey);
    console.log(`\nBalance after: ${balanceAfter / 1e9} SOL`);
    console.log(`Rent reclaimed: ${(balanceAfter - balanceBefore) / 1e9} SOL`);
}

main().catch(console.error);
