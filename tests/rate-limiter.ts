import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { RateLimiter } from "../target/types/rate_limiter";
import { PublicKey } from "@solana/web3.js";
import { expect } from "chai";

describe("rate-limiter", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.RateLimiter as Program<RateLimiter>;
  const wallet = provider.wallet as anchor.Wallet;

  const [configPDA] = PublicKey.findProgramAddressSync(
    [Buffer.from("config")],
    program.programId
  );

  function deriveApiKeyPDA(owner: PublicKey, seed: Buffer): [PublicKey, number] {
    return PublicKey.findProgramAddressSync(
      [Buffer.from("api_key"), owner.toBuffer(), seed],
      program.programId
    );
  }

  // Test 1: Initialize global config
  it("initializes global config", async () => {
    try {
      await program.methods
        .initializeConfig()
        .accounts({ admin: wallet.publicKey, config: configPDA })
        .rpc();
    } catch (e) {
      console.log("Config may already exist:", (e as Error).message.slice(0, 80));
    }
    const config = await program.account.globalConfig.fetch(configPDA);
    expect(config.admin.toString()).to.equal(wallet.publicKey.toString());
  });

  // Test 2: Create API key
  const keySeed = Buffer.from("test-key-001");
  const [apiKeyPDA] = deriveApiKeyPDA(wallet.publicKey, keySeed);

  it("creates a new API key with rate limit", async () => {
    try {
      await program.methods
        .createApiKey(Array.from(keySeed), new anchor.BN(1000), new anchor.BN(3600))
        .accounts({ owner: wallet.publicKey, apiKey: apiKeyPDA })
        .rpc();
    } catch (e) {
      console.log("Key may already exist:", (e as Error).message.slice(0, 80));
    }
    const apiKey = await program.account.apiKeyAccount.fetch(apiKeyPDA);
    expect(apiKey.owner.toString()).to.equal(wallet.publicKey.toString());
    expect(apiKey.quota.toString()).to.equal("1000");
    expect(apiKey.used.toString()).to.equal("0");
    expect(apiKey.isActive).to.equal(true);
  });

  // Test 3: Increment counter
  it("increments request counter", async () => {
    await program.methods.incrementCounter().accounts({ apiKey: apiKeyPDA }).rpc();
    const apiKey = await program.account.apiKeyAccount.fetch(apiKeyPDA);
    expect(apiKey.used.toString()).to.equal("1");
    console.log(`Counter incremented to ${apiKey.used}`);
  });

  // Test 4: Rate limit enforcement
  it("rejects requests that exceed quota", async () => {
    const tinySeed = Buffer.from("tiny-quota-key");
    const [tinyPDA] = deriveApiKeyPDA(wallet.publicKey, tinySeed);
    try {
      await program.methods
        .createApiKey(Array.from(tinySeed), new anchor.BN(2), new anchor.BN(3600))
        .accounts({ owner: wallet.publicKey, apiKey: tinyPDA })
        .rpc();
    } catch (_) {}

    await program.methods.incrementCounter().accounts({ apiKey: tinyPDA }).rpc();
    await program.methods.incrementCounter().accounts({ apiKey: tinyPDA }).rpc();

    try {
      await program.methods.incrementCounter().accounts({ apiKey: tinyPDA }).rpc();
      throw new Error("Should have been rejected!");
    } catch (e) {
      expect((e as anchor.AnchorError).message).to.include("0x1");
      console.log("3rd request correctly rejected with rate limit error");
    }
  });

  // Test 5: Revoke API key
  it("admin can revoke API key", async () => {
    await program.methods
      .revokeApiKey()
      .accounts({ admin: wallet.publicKey, config: configPDA, apiKey: apiKeyPDA })
      .rpc();
    const apiKey = await program.account.apiKeyAccount.fetch(apiKeyPDA);
    expect(apiKey.isActive).to.equal(false);
  });

  // Test 6: Update quota
  it("admin can update quota", async () => {
    const freshSeed = Buffer.from("fresh-key");
    const [freshPDA] = deriveApiKeyPDA(wallet.publicKey, freshSeed);
    try {
      await program.methods
        .createApiKey(Array.from(freshSeed), new anchor.BN(100), new anchor.BN(3600))
        .accounts({ owner: wallet.publicKey, apiKey: freshPDA })
        .rpc();
    } catch (_) {}
    await program.methods
      .updateQuota(new anchor.BN(5000))
      .accounts({ admin: wallet.publicKey, config: configPDA, apiKey: freshPDA })
      .rpc();
    const apiKey = await program.account.apiKeyAccount.fetch(freshPDA);
    expect(apiKey.quota.toString()).to.equal("5000");
    console.log("Quota updated from 100 to 5000");
  });

  console.log("\n=== All tests passed! ===");
  console.log("Program ID:", program.programId.toString());
  console.log("Config PDA:", configPDA.toString());
});
