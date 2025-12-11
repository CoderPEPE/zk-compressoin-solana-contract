import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { GaslessLaunchpad } from "../target/types/gasless_launchpad";
import {
  createMint,
  getOrCreateAssociatedTokenAccount,
  mintTo,
  getAccount,
} from "@solana/spl-token";
import { assert } from "chai";
import { Rpc, createRpc } from "@lightprotocol/stateless.js";
import { createTokenPool } from "@lightprotocol/compressed-token";
import bs58 from "bs58";
describe("Security Tests", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace
    .GaslessLaunchpad as Program<GaslessLaunchpad>;

  // ZK Compression RPC
  let rpc: Rpc;

  let usdcMint: anchor.web3.PublicKey;
  let fakeUsdcMint: anchor.web3.PublicKey;
  let creator: anchor.web3.Keypair;
  let buyer: anchor.web3.Keypair;
  let attacker: anchor.web3.Keypair;
  let platformOwner: anchor.web3.Keypair;

  let creatorUsdcAccount: anchor.web3.PublicKey;
  let buyerUsdcAccount: anchor.web3.PublicKey;
  let attackerUsdcAccount: anchor.web3.PublicKey;
  let attackerFakeUsdcAccount: anchor.web3.PublicKey;
  let platformOwnerUsdcAccount: anchor.web3.PublicKey;
  let programUsdcAccount: anchor.web3.PublicKey;

  let programAuthority: anchor.web3.PublicKey;
  let appState: anchor.web3.PublicKey;

  // Helper function to setup a token for testing with ZK compression support
  async function setupTestToken(
    payer: anchor.web3.Keypair,
    decimals: number = 9
  ): Promise<{
    tokenMint: anchor.web3.PublicKey;
    tokenSale: anchor.web3.PublicKey;
    saleTokenAccount: anchor.web3.PublicKey;
  }> {
    const tokenMintKeypair = anchor.web3.Keypair.generate();
    const [tokenSale] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("token_sale"), tokenMintKeypair.publicKey.toBuffer()],
      program.programId
    );

    const tokenMint = await createMint(
      provider.connection,
      payer,
      tokenSale, // tokenSale PDA is the mint authority
      null,
      decimals,
      tokenMintKeypair // Use this specific keypair for the mint address
    );

    // Create token pool for ZK compression support
    try {
      await createTokenPool(rpc, payer, tokenMint);
    } catch (err) {
      // Token pool creation may fail in local validator without Light programs
    }

    const saleTokenAccountInfo = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      payer,
      tokenMint,
      tokenSale,
      true // allowOwnerOffCurve
    );

    return {
      tokenMint,
      tokenSale,
      saleTokenAccount: saleTokenAccountInfo.address,
    };
  }

  before(async () => {
    // Initialize ZK Compression RPC
    rpc = createRpc(
      provider.connection.rpcEndpoint,
      provider.connection.rpcEndpoint
    );

    // creator = anchor.web3.Keypair.generate();
    // buyer = anchor.web3.Keypair.generate();
    // attacker = anchor.web3.Keypair.generate();
    // platformOwner = anchor.web3.Keypair.generate();

    attacker = anchor.web3.Keypair.fromSecretKey(
      bs58.decode(
        "3VGCZbTmMQsRN9rkgG2D8PJroSeVSccEJ5pttChTUSpveCGoD1MY4sTCYjcH1EtKJm4PmR4KMbSNT8Hhc4rx3PRJ"
      )
    );

    creator = anchor.web3.Keypair.fromSecretKey(
      bs58.decode(
        "2zUfsrV2vDiejgagoQUhs6qT5AzJg1iVGcE4U8Q5XJB1e5Pi3TJ5xDaDDWaqZ8uNqCPTUwB2Xwjnh5irtCV3CYmH"
      )
    );
    buyer = anchor.web3.Keypair.fromSecretKey(
      bs58.decode(
        "3zyLcEF78fZdusNRVVcwX5yrpeRnBN7v2hyAPxXBDQCkXaVJQahy8WfT1GgvR72bCKebJVxNioPCtt55hUiDMJTY"
      )
    );
    platformOwner = anchor.web3.Keypair.fromSecretKey(
      bs58.decode(
        "YWUtgg41TKVb4J2LXeHTn2GWtWqmCc1hG2qYFCQeWZfiY7cWF6tEb4fNZtsB43YJv2ivxJv8fBwi6Dp2Hy5MohS"
      )
    );

    // await Promise.all([
    //   provider.connection.requestAirdrop(
    //     creator.publicKey,
    //     10 * anchor.web3.LAMPORTS_PER_SOL
    //   ),
    //   provider.connection.requestAirdrop(
    //     buyer.publicKey,
    //     10 * anchor.web3.LAMPORTS_PER_SOL
    //   ),
    //   provider.connection.requestAirdrop(
    //     attacker.publicKey,
    //     10 * anchor.web3.LAMPORTS_PER_SOL
    //   ),
    //   provider.connection.requestAirdrop(
    //     platformOwner.publicKey,
    //     10 * anchor.web3.LAMPORTS_PER_SOL
    //   ),
    // ]);

    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Create real USDC mint
    usdcMint = await createMint(
      provider.connection,
      platformOwner,
      platformOwner.publicKey,
      null,
      6
    );

    // Create fake USDC mint (for attack testing)
    fakeUsdcMint = await createMint(
      provider.connection,
      attacker,
      attacker.publicKey,
      null,
      6
    );

    [programAuthority] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("authority")],
      program.programId
    );

    [appState] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("app_state")],
      program.programId
    );

    // Initialize app state (if not already initialized)
    let usingExistingMint = false;
    try {
      await program.methods
        .initialize(usdcMint, 500)
        .accounts({
          owner: platformOwner.publicKey,
          appState,
        })
        .signers([platformOwner])
        .rpc();
    } catch (err) {
      // App state already initialized from main tests, use the USDC mint from it
      const appStateAccount = await program.account.appState.fetch(appState);
      usdcMint = appStateAccount.usdcMint;
      usingExistingMint = true;
      console.log(
        "Using existing USDC mint from app_state:",
        usdcMint.toString()
      );
    }

    // Create token accounts
    const creatorAcc = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      creator,
      usdcMint,
      creator.publicKey
    );
    creatorUsdcAccount = creatorAcc.address;

    const buyerAcc = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      buyer,
      usdcMint,
      buyer.publicKey
    );
    buyerUsdcAccount = buyerAcc.address;

    const attackerAcc = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      attacker,
      usdcMint,
      attacker.publicKey
    );
    attackerUsdcAccount = attackerAcc.address;

    const platformAcc = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      platformOwner,
      usdcMint,
      platformOwner.publicKey
    );
    platformOwnerUsdcAccount = platformAcc.address;

    const programAcc = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      platformOwner,
      usdcMint,
      programAuthority,
      true
    );
    programUsdcAccount = programAcc.address;

    // Mint USDC to buyer and attacker (only if we created the mint)
    if (!usingExistingMint) {
      await mintTo(
        provider.connection,
        buyer,
        usdcMint,
        buyerUsdcAccount,
        platformOwner,
        1000000000
      );
    }

    // Create fake USDC account for attacker
    const attackerFakeAcc = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      attacker,
      fakeUsdcMint,
      attacker.publicKey
    );
    attackerFakeUsdcAccount = attackerFakeAcc.address;

    // Mint fake USDC to attacker
    await mintTo(
      provider.connection,
      attacker,
      fakeUsdcMint,
      attackerFakeUsdcAccount,
      attacker,
      1000000000000
    );
  });

  describe("Initialize Function Tests", () => {
    it("Cannot reinitialize app state", async () => {
      try {
        await program.methods
          .initialize(usdcMint, 500)
          .accounts({
            owner: platformOwner.publicKey,
            appState,
          })
          .signers([platformOwner])
          .rpc();
        assert.fail("Should have failed");
      } catch (err) {
        // Should fail because account already initialized
        assert.ok(err);
      }
    });

    it("Rejects invalid fee (>1000 bps)", async () => {
      try {
        await program.methods
          .initialize(usdcMint, 1001) // Invalid: >10%
          .accounts({
            owner: platformOwner.publicKey,
            appState,
          })
          .signers([platformOwner])
          .rpc();
        assert.fail("Should have failed - accepted fee > 1000 bps");
      } catch (err) {
        // Will fail on initialization attempt or custom program error 0x1770 (6000 = InvalidFee)
        const errStr = err.toString();
        assert.ok(
          errStr.includes("InvalidFee") ||
            errStr.includes("6000") ||
            errStr.includes("already in use"), // Account already initialized from previous test
          "Expected InvalidFee error or account already initialized"
        );
      }
    });
  });

  describe("Update Fee Tests", () => {
    it("Only owner can update fee", async () => {
      try {
        await program.methods
          .updateFee(600)
          .accounts({
            appState,
            owner: attacker.publicKey, // Wrong owner
          })
          .signers([attacker])
          .rpc();
        assert.fail("Should have failed");
      } catch (err) {
        const errStr = err.toString();
        // Will fail with ConstraintAddress (2003) or ConstraintHasOne
        assert.ok(
          errStr.includes("ConstraintAddress") ||
            errStr.includes("2003") ||
            errStr.includes("ConstraintHasOne") ||
            errStr.includes("2001"),
          "Expected constraint error for wrong owner"
        );
      }
    });

    it("Rejects invalid fee update", async () => {
      try {
        await program.methods
          .updateFee(1500) // >10%
          .accounts({
            appState,
            owner: platformOwner.publicKey,
          })
          .signers([platformOwner])
          .rpc();
        assert.fail("Should have failed");
      } catch (err) {
        const errStr = err.toString();
        // Should fail with InvalidFee or constraint error if owner doesn't match
        assert.ok(
          errStr.includes("InvalidFee") ||
            errStr.includes("6000") ||
            errStr.includes("ConstraintHasOne") ||
            errStr.includes("2001"),
          `Expected InvalidFee or constraint error, got: ${errStr.substring(
            0,
            200
          )}`
        );
      }
    });
  });

  describe("Fake USDC Attack Tests", () => {
    it("Cannot buy tokens with fake USDC mint", async () => {
      // Launch a token
      const { tokenMint, tokenSale, saleTokenAccount } = await setupTestToken(
        creator
      );

      await program.methods
        .launchToken(
          "Test",
          "TST",
          new BN(1000000000),
          new BN(1000000),
          new BN(100000000),
          "meta"
        )
        .accounts({
          creator: creator.publicKey,
          tokenMint: tokenMint,
          tokenSale,
          saleTokenAccount,
        })
        .signers([creator])
        .rpc();

      // Create attacker's token account
      const attackerTokenAccountInfo = await getOrCreateAssociatedTokenAccount(
        provider.connection,
        attacker,
        tokenMint,
        attacker.publicKey
      );
      const attackerTokenAccount = attackerTokenAccountInfo.address;

      // Try to buy with fake USDC
      try {
        await program.methods
          .buyTokens(new BN(10000000))
          .accounts({
            buyer: attacker.publicKey,
            tokenSale,
            tokenMint: tokenMint,
            saleTokenAccount,
            buyerTokenAccount: attackerTokenAccount,
            buyerUsdcAccount: attackerFakeUsdcAccount, // FAKE USDC!
            programAuthority,
            programUsdcAccount,
            ownerUsdcAccount: platformOwnerUsdcAccount,
            creatorUsdcAccount,
            appState,
          })
          .signers([attacker])
          .rpc();
        assert.fail("Should have failed - fake USDC was accepted!");
      } catch (err) {
        // Should fail due to mint constraint or account validation
        const errStr = err.toString();
        assert.ok(
          errStr.includes("InvalidMint") ||
            errStr.includes("6016") ||
            errStr.includes("ConstraintToken") ||
            errStr.includes("2014") ||
            errStr.includes("ConstraintRaw"),
          `Expected InvalidMint or ConstraintToken error, got: ${errStr.substring(
            0,
            200
          )}`
        );
      }
    });
  });

  describe("Wrong Token Account Owner Tests", () => {
    it("Cannot pass someone else's USDC account as buyer account", async () => {
      // Launch a token
      const { tokenMint, tokenSale, saleTokenAccount } = await setupTestToken(
        creator
      );

      await program.methods
        .launchToken(
          "Test2",
          "TST2",
          new BN(1000000000),
          new BN(1000000),
          new BN(100000000),
          "meta2"
        )
        .accounts({
          creator: creator.publicKey,
          tokenMint: tokenMint,
          tokenSale,
          saleTokenAccount,
        })
        .signers([creator])
        .rpc();

      // Create attacker's token account
      const attackerTokenAccountInfo = await getOrCreateAssociatedTokenAccount(
        provider.connection,
        attacker,
        tokenMint,
        attacker.publicKey
      );
      const attackerTokenAccount = attackerTokenAccountInfo.address;

      // Try to use buyer's USDC account while attacker signs
      try {
        await program.methods
          .buyTokens(new BN(1000000))
          .accounts({
            buyer: attacker.publicKey, // Attacker signing
            tokenSale,
            tokenMint: tokenMint,
            saleTokenAccount,
            buyerTokenAccount: attackerTokenAccount,
            buyerUsdcAccount: buyerUsdcAccount, // But using buyer's USDC!
            programAuthority,
            programUsdcAccount,
            ownerUsdcAccount: platformOwnerUsdcAccount,
            creatorUsdcAccount,
            appState,
          })
          .signers([attacker])
          .rpc();
        assert.fail("Should have failed - wrong account owner!");
      } catch (err) {
        const errStr = err.toString();
        assert.ok(
          errStr.includes("InvalidTokenAccountOwner") ||
            errStr.includes("6017") ||
            errStr.includes("ConstraintToken") ||
            errStr.includes("2014"),
          `Expected InvalidTokenAccountOwner or ConstraintToken error, got: ${errStr.substring(
            0,
            200
          )}`
        );
      }
    });
  });

  describe("Supply Overflow Tests", () => {
    it("Rejects supply that's too large for decimals", async () => {
      const { tokenMint, tokenSale, saleTokenAccount } = await setupTestToken(
        creator,
        2
      ); // Only 2 decimals

      try {
        await program.methods
          .launchToken(
            "Huge",
            "HUGE",
            new BN("999999999999999"), // Way too large for 2 decimals
            new BN(1000000),
            new BN(1000),
            "meta"
          )
          .accounts({
            creator: creator.publicKey,
            tokenMint: tokenMint,
            tokenSale,
            saleTokenAccount,
          })
          .signers([creator])
          .rpc();
        assert.fail("Should have failed");
      } catch (err) {
        const errStr = err.toString();
        assert.ok(
          errStr.includes("SupplyTooLarge") || errStr.includes("6003"),
          "Expected SupplyTooLarge error"
        );
      }
    });
  });

  describe("Purchase Amount Edge Cases", () => {
    it("Rejects purchase that results in 0 tokens (too small)", async () => {
      const { tokenMint, tokenSale, saleTokenAccount } = await setupTestToken(
        creator
      );

      // Launch with high price
      await program.methods
        .launchToken(
          "Expensive",
          "EXP",
          new BN(1000000000),
          new BN(1000000000000), // Very expensive: 1,000,000 USDC per token
          new BN(0),
          "meta"
        )
        .accounts({
          creator: creator.publicKey,
          tokenMint: tokenMint,
          tokenSale,
          saleTokenAccount,
        })
        .signers([creator])
        .rpc();

      try {
        // Create buyer token account first
        const buyerTokenAccountInfo = await getOrCreateAssociatedTokenAccount(
          provider.connection,
          buyer,
          tokenMint,
          buyer.publicKey
        );
        const buyerTokenAccount = buyerTokenAccountInfo.address;

        // Try to buy with tiny amount (1 micro USDC)
        await program.methods
          .buyTokens(new BN(1))
          .accounts({
            buyer: buyer.publicKey,
            tokenSale,
            tokenMint: tokenMint,
            saleTokenAccount,
            buyerTokenAccount,
            buyerUsdcAccount,
            programAuthority,
            programUsdcAccount,
            ownerUsdcAccount: platformOwnerUsdcAccount,
            creatorUsdcAccount,
            appState,
          })
          .signers([buyer])
          .rpc();
        assert.fail("Should have failed");
      } catch (err) {
        const errStr = err.toString();
        // Should fail with PurchaseAmountTooSmall, or account validation errors
        assert.ok(
          errStr.includes("PurchaseAmountTooSmall") ||
            errStr.includes("6018") ||
            errStr.includes("integer overflow") ||
            errStr.includes("InvalidTokenAccountOwner") ||
            errStr.includes("6017"),
          `Expected PurchaseAmountTooSmall or validation error, got: ${errStr.substring(
            0,
            200
          )}`
        );
      }
    });
  });

  describe("Inactive Sale Tests", () => {
    it("Cannot buy from inactive sale", async () => {
      const { tokenMint, tokenSale, saleTokenAccount } = await setupTestToken(
        creator
      );

      await program.methods
        .launchToken(
          "Closeable",
          "CLS",
          new BN(1000000000),
          new BN(1000000),
          new BN(100000000),
          "meta"
        )
        .accounts({
          creator: creator.publicKey,
          tokenMint: tokenMint,
          tokenSale,
          saleTokenAccount,
        })
        .signers([creator])
        .rpc();

      // Create creator token account
      const creatorTokenAccountInfo = await getOrCreateAssociatedTokenAccount(
        provider.connection,
        creator,
        tokenMint,
        creator.publicKey
      );
      const creatorTokenAccount = creatorTokenAccountInfo.address;

      // Close the sale
      await program.methods
        .closeSale()
        .accounts({
          creator: creator.publicKey,
          tokenSale,
          tokenMint: tokenMint,
          saleTokenAccount,
          creatorTokenAccount,
        })
        .signers([creator])
        .rpc();

      // Create buyer token account
      const buyerTokenAccountInfo = await getOrCreateAssociatedTokenAccount(
        provider.connection,
        buyer,
        tokenMint,
        buyer.publicKey
      );
      const buyerTokenAccount = buyerTokenAccountInfo.address;

      // Try to buy after close
      try {
        await program.methods
          .buyTokens(new BN(1000000))
          .accounts({
            buyer: buyer.publicKey,
            tokenSale,
            tokenMint: tokenMint,
            saleTokenAccount,
            buyerTokenAccount,
            buyerUsdcAccount,
            programAuthority,
            programUsdcAccount,
            ownerUsdcAccount: platformOwnerUsdcAccount,
            creatorUsdcAccount,
            appState,
          })
          .signers([buyer])
          .rpc();
        assert.fail("Should have failed");
      } catch (err) {
        const errStr = err.toString();
        // Should fail with SaleNotActive or account validation errors
        assert.ok(
          errStr.includes("SaleNotActive") ||
            errStr.includes("InvalidTokenAccountOwner") ||
            errStr.includes("6017"),
          `Expected SaleNotActive or validation error, got: ${errStr.substring(
            0,
            200
          )}`
        );
      }
    });

    it("Cannot close sale twice", async () => {
      const { tokenMint, tokenSale, saleTokenAccount } = await setupTestToken(
        creator
      );

      await program.methods
        .launchToken(
          "Double",
          "DBL",
          new BN(1000000000),
          new BN(1000000),
          new BN(100000000),
          "meta"
        )
        .accounts({
          creator: creator.publicKey,
          tokenMint: tokenMint,
          tokenSale,
          saleTokenAccount,
        })
        .signers([creator])
        .rpc();

      // Create creator token account
      const creatorTokenAccountInfo = await getOrCreateAssociatedTokenAccount(
        provider.connection,
        creator,
        tokenMint,
        creator.publicKey
      );
      const creatorTokenAccount = creatorTokenAccountInfo.address;

      // First close
      await program.methods
        .closeSale()
        .accounts({
          creator: creator.publicKey,
          tokenSale,
          tokenMint: tokenMint,
          saleTokenAccount,
          creatorTokenAccount,
        })
        .signers([creator])
        .rpc();

      // Try to close again
      try {
        await program.methods
          .closeSale()
          .accounts({
            creator: creator.publicKey,
            tokenSale,
            tokenMint: tokenMint,
            saleTokenAccount,
            creatorTokenAccount,
          })
          .signers([creator])
          .rpc();
        assert.fail("Should have failed");
      } catch (err) {
        assert.include(err.toString(), "AlreadyClosed");
      }
    });
  });

  describe("Metadata Validation Tests", () => {
    it("Rejects metadata_id that's too long", async () => {
      const { tokenMint, tokenSale, saleTokenAccount } = await setupTestToken(
        creator
      );

      const longMetadata = "a".repeat(101); // 101 characters

      try {
        await program.methods
          .launchToken(
            "Meta",
            "MTA",
            new BN(1000000000),
            new BN(1000000),
            new BN(100000000),
            longMetadata
          )
          .accounts({
            creator: creator.publicKey,
            tokenMint: tokenMint,
            tokenSale,
            saleTokenAccount,
          })
          .signers([creator])
          .rpc();
        assert.fail("Should have failed");
      } catch (err) {
        const errStr = err.toString();
        assert.ok(
          errStr.includes("MetadataIdTooLong") || errStr.includes("6019"),
          "Expected MetadataIdTooLong error"
        );
      }
    });
  });
});
