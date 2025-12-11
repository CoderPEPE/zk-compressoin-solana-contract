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

// Helper function to log transaction gas costs
async function logGasCost(
  connection: anchor.web3.Connection,
  txSignature: string,
  operationName: string
) {
  const SOL_PRICE_USD = 130; // Current SOL price
  const LAMPORTS_PER_SOL = 1_000_000_000;

  try {
    // Wait a bit for transaction to be confirmed
    await new Promise(resolve => setTimeout(resolve, 500));

    // Fetch transaction details
    const txDetails = await connection.getTransaction(txSignature, {
      maxSupportedTransactionVersion: 0,
      commitment: "confirmed",
    });

    if (txDetails && txDetails.meta) {
      const fee = txDetails.meta.fee;
      const computeUnitsUsed = txDetails.meta.computeUnitsConsumed || 0;
      const solCost = fee / LAMPORTS_PER_SOL;
      const usdCost = solCost * SOL_PRICE_USD;

      console.log(`\n💰 Gas Cost - ${operationName}:`);
      console.log(`   Compute Units: ${computeUnitsUsed.toLocaleString()}`);
      console.log(`   Fee: ${fee.toLocaleString()} lamports (${solCost.toFixed(9)} SOL)`);
      console.log(`   USD Cost: $${usdCost.toFixed(6)} (@ $${SOL_PRICE_USD}/SOL)\n`);
    } else {
      console.log(`⚠️  Could not fetch gas cost for ${operationName} - transaction details not available yet`);
    }
  } catch (err) {
    console.error(`❌ Failed to fetch gas cost for ${operationName}:`, err.message);
  }
}

describe("gasless-launchpad", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.GaslessLaunchpad as Program<GaslessLaunchpad>;

  // Test accounts
  let usdcMint: anchor.web3.PublicKey;
  let creator: anchor.web3.Keypair;
  let buyer: anchor.web3.Keypair;
  let platformOwner: anchor.web3.Keypair;

  // Token accounts
  let creatorUsdcAccount: anchor.web3.PublicKey;
  let buyerUsdcAccount: anchor.web3.PublicKey;
  let platformOwnerUsdcAccount: anchor.web3.PublicKey;
  let programUsdcAccount: anchor.web3.PublicKey;

  // Program authority PDA
  let programAuthority: anchor.web3.PublicKey;
  let programAuthorityBump: number;

  // App state PDA
  let appState: anchor.web3.PublicKey;

  // Helper function to setup a token for testing
  async function setupTestToken(
    payer: anchor.web3.Keypair,
    decimals: number = 9
  ): Promise<{ tokenMint: anchor.web3.PublicKey, tokenSale: anchor.web3.PublicKey, saleTokenAccount: anchor.web3.PublicKey }> {
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

    const saleTokenAccountInfo = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      payer,
      tokenMint,
      tokenSale,
      true // allowOwnerOffCurve
    );

    return { tokenMint, tokenSale, saleTokenAccount: saleTokenAccountInfo.address };
  }

  before(async () => {
    // Initialize test keypairs
    creator = anchor.web3.Keypair.generate();
    buyer = anchor.web3.Keypair.generate();
    platformOwner = anchor.web3.Keypair.generate();

    // Airdrop SOL to test accounts
    await Promise.all([
      provider.connection.requestAirdrop(creator.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL),
      provider.connection.requestAirdrop(buyer.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL),
      provider.connection.requestAirdrop(platformOwner.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL),
    ]);

    // Wait for airdrops to confirm
    await new Promise(resolve => setTimeout(resolve, 1000));

    // Create USDC mock token
    usdcMint = await createMint(
      provider.connection,
      platformOwner,
      platformOwner.publicKey,
      null,
      6 // USDC has 6 decimals
    );

    // Find program authority PDA
    [programAuthority, programAuthorityBump] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("authority")],
      program.programId
    );

    // Find app state PDA
    [appState] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("app_state")],
      program.programId
    );

    // Initialize app state
    const initTx = await program.methods
      .initialize(usdcMint, 500) // 5% platform fee
      .accounts({
        owner: platformOwner.publicKey,
        appState,
      })
      .signers([platformOwner])
      .rpc();

    console.log("Initialize tx:", initTx);
    await logGasCost(provider.connection, initTx, "Initialize App State");

    // Create token accounts
    const creatorAccount = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      creator,
      usdcMint,
      creator.publicKey
    );
    creatorUsdcAccount = creatorAccount.address;

    const buyerAccount = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      buyer,
      usdcMint,
      buyer.publicKey
    );
    buyerUsdcAccount = buyerAccount.address;

    const platformAccount = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      platformOwner,
      usdcMint,
      platformOwner.publicKey
    );
    platformOwnerUsdcAccount = platformAccount.address;

    const programAccount = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      platformOwner,
      usdcMint,
      programAuthority,
      true
    );
    programUsdcAccount = programAccount.address;

    // Mint USDC to buyer
    await mintTo(
      provider.connection,
      buyer,
      usdcMint,
      buyerUsdcAccount,
      platformOwner,
      1000000000 // 1000 USDC
    );
  });

  describe("Launch Token", () => {
    it("Successfully launches a paid token", async () => {
      const name = "Test Token";
      const symbol = "TEST";
      const supply = new BN(1000000000); // 1 token with 9 decimals
      const pricePerToken = new BN(1000000); // 1 USDC per token
      const limitPerMint = new BN(100000000); // 0.1 token limit
      const metadataId = "metadata123";

      const { tokenMint, tokenSale, saleTokenAccount } = await setupTestToken(creator);

      const tx = await program.methods
        .launchToken(name, symbol, supply, pricePerToken, limitPerMint, metadataId)
        .accounts({
          creator: creator.publicKey,
          tokenMint: tokenMint,
          tokenSale,
          saleTokenAccount,
        })
        .signers([creator])
        .rpc();

      console.log("Launch token tx:", tx);
      await logGasCost(provider.connection, tx, "Launch Token");

      // Verify token sale account
      const saleAccount = await program.account.tokenSale.fetch(tokenSale);
      assert.equal(saleAccount.creator.toString(), creator.publicKey.toString());
      assert.equal(saleAccount.pricePerToken.toString(), pricePerToken.toString());
      assert.equal(saleAccount.supplyForSale.toString(), supply.toString());
      assert.equal(saleAccount.tokensSold.toString(), "0");
      assert.isTrue(saleAccount.active);
      assert.equal(saleAccount.metadataId, metadataId);
      assert.equal(saleAccount.decimals, 9);

      // Verify tokens were minted
      const saleTokenAccountInfo = await getAccount(provider.connection, saleTokenAccount);
      assert.equal(saleTokenAccountInfo.amount.toString(), supply.toString());
    });

    it("Successfully launches a free mint token", async () => {
      const name = "Free Token";
      const symbol = "FREE";
      const supply = new BN(1000000);
      const pricePerToken = new BN(0); // Free
      const limitPerMint = new BN(100); // Must set limit for free mints
      const metadataId = "free123";

      const { tokenMint, tokenSale, saleTokenAccount } = await setupTestToken(creator);

      await program.methods
        .launchToken(name, symbol, supply, pricePerToken, limitPerMint, metadataId)
        .accounts({
          creator: creator.publicKey,
          tokenMint: tokenMint,
          tokenSale,
          saleTokenAccount,
        })
        .signers([creator])
        .rpc();

      const saleAccount = await program.account.tokenSale.fetch(tokenSale);
      assert.equal(saleAccount.pricePerToken.toString(), "0");
      assert.equal(saleAccount.limitPerMint.toString(), limitPerMint.toString());
    });

    it("Fails to launch with invalid name length", async () => {
      const name = ""; // Invalid empty name
      const symbol = "TEST";
      const supply = new BN(1000000);
      const pricePerToken = new BN(1000000);
      const limitPerMint = new BN(10000);
      const metadataId = "meta";

      const { tokenMint, tokenSale, saleTokenAccount } = await setupTestToken(creator);

      try {
        await program.methods
          .launchToken(name, symbol, supply, pricePerToken, limitPerMint, metadataId)
          .accounts({
            creator: creator.publicKey,
            tokenMint: tokenMint,
            tokenSale,
            saleTokenAccount,
          })
          .signers([creator])
          .rpc();
        assert.fail("Should have failed with invalid name");
      } catch (err) {
        assert.include(err.toString(), "InvalidNameLength");
      }
    });

    it("Fails free mint without limit per mint", async () => {
      const name = "Test";
      const symbol = "TST";
      const supply = new BN(1000000);
      const pricePerToken = new BN(0);
      const limitPerMint = new BN(0); // Invalid for free mint
      const metadataId = "meta";

      const { tokenMint, tokenSale, saleTokenAccount } = await setupTestToken(creator);

      try {
        await program.methods
          .launchToken(name, symbol, supply, pricePerToken, limitPerMint, metadataId)
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
        assert.include(err.toString(), "FreeMintRequiresLimit");
      }
    });
  });

  describe("Buy Tokens", () => {
    let testTokenMint: anchor.web3.PublicKey;
    let testTokenSale: anchor.web3.PublicKey;
    let saleTokenAccount: anchor.web3.PublicKey;

    before(async () => {
      // Launch a test token for buying
      const setup = await setupTestToken(creator);
      testTokenMint = setup.tokenMint;
      testTokenSale = setup.tokenSale;
      saleTokenAccount = setup.saleTokenAccount;

      await program.methods
        .launchToken(
          "Buy Test",
          "BUY",
          new BN(1000000000000), // 1,000 tokens (9 decimals) - max allowed
          new BN(1000000), // 1 USDC per token
          new BN(100000000000), // 100 tokens limit
          "buy123"
        )
        .accounts({
          creator: creator.publicKey,
          tokenMint: testTokenMint,
          tokenSale: testTokenSale,
          saleTokenAccount,
        })
        .signers([creator])
        .rpc();
    });

    it("Successfully buys tokens with USDC", async () => {
      const usdcAmount = new BN(10000000); // 10 USDC
      const expectedTokens = new BN(10000000000); // 10,000 tokens (9 decimals)

      // Create buyer token account
      const buyerTokenAccountInfo = await getOrCreateAssociatedTokenAccount(
        provider.connection,
        buyer,
        testTokenMint,
        buyer.publicKey
      );
      const buyerTokenAccount = buyerTokenAccountInfo.address;

      // Get initial balances
      const initialBuyerUsdc = await getAccount(provider.connection, buyerUsdcAccount);
      const initialCreatorUsdc = await getAccount(provider.connection, creatorUsdcAccount);
      const initialPlatformUsdc = await getAccount(provider.connection, platformOwnerUsdcAccount);

      const tx = await program.methods
        .buyTokens(usdcAmount)
        .accounts({
          buyer: buyer.publicKey,
          tokenSale: testTokenSale,
          tokenMint: testTokenMint,
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

      console.log("Buy tokens tx:", tx);
      await logGasCost(provider.connection, tx, "Buy Tokens (10 USDC)");

      // Verify buyer received tokens
      const finalBuyerTokenAccount = await getAccount(provider.connection, buyerTokenAccount);
      assert.equal(finalBuyerTokenAccount.amount.toString(), expectedTokens.toString());

      // Verify USDC distribution
      const finalBuyerUsdc = await getAccount(provider.connection, buyerUsdcAccount);
      const finalCreatorUsdc = await getAccount(provider.connection, creatorUsdcAccount);
      const finalPlatformUsdc = await getAccount(provider.connection, platformOwnerUsdcAccount);

      // Buyer paid
      assert.equal(
        (Number(initialBuyerUsdc.amount) - Number(finalBuyerUsdc.amount)).toString(),
        usdcAmount.toString()
      );

      // Platform got 5% fee
      const expectedFee = usdcAmount.mul(new BN(500)).div(new BN(10000));
      assert.equal(
        (Number(finalPlatformUsdc.amount) - Number(initialPlatformUsdc.amount)).toString(),
        expectedFee.toString()
      );

      // Creator got 95%
      const expectedCreatorShare = usdcAmount.sub(expectedFee);
      assert.equal(
        (Number(finalCreatorUsdc.amount) - Number(initialCreatorUsdc.amount)).toString(),
        expectedCreatorShare.toString()
      );

      // Verify sale state updated
      const saleAccount = await program.account.tokenSale.fetch(testTokenSale);
      assert.equal(saleAccount.tokensSold.toString(), expectedTokens.toString());
    });

    it("Successfully mints free tokens", async () => {
      // Launch free mint token
      const { tokenMint: freeTokenMint, tokenSale: freeTokenSale, saleTokenAccount: freeSaleTokenAccount } = await setupTestToken(creator);

      await program.methods
        .launchToken(
          "Free Token",
          "FREE",
          new BN(1000000),
          new BN(0), // Free
          new BN(100), // Limit per mint
          "free"
        )
        .accounts({
          creator: creator.publicKey,
          tokenMint: freeTokenMint,
          tokenSale: freeTokenSale,
          saleTokenAccount: freeSaleTokenAccount,
        })
        .signers([creator])
        .rpc();

      // Create buyer token account
      const buyerFreeTokenAccountInfo = await getOrCreateAssociatedTokenAccount(
        provider.connection,
        buyer,
        freeTokenMint,
        buyer.publicKey
      );
      const buyerFreeTokenAccount = buyerFreeTokenAccountInfo.address;

      await program.methods
        .buyTokens(new BN(0))
        .accounts({
          buyer: buyer.publicKey,
          tokenSale: freeTokenSale,
          tokenMint: freeTokenMint,
          saleTokenAccount: freeSaleTokenAccount,
          buyerTokenAccount: buyerFreeTokenAccount,
          buyerUsdcAccount,
          programAuthority,
          programUsdcAccount,
          ownerUsdcAccount: platformOwnerUsdcAccount,
          creatorUsdcAccount,
          appState,
        })
        .signers([buyer])
        .rpc();

      const buyerTokenAccountInfo = await getAccount(provider.connection, buyerFreeTokenAccount);
      assert.equal(buyerTokenAccountInfo.amount.toString(), "100");
    });

    it("Fails when exceeding mint limit", async () => {
      const usdcAmount = new BN(200000000); // 200 USDC = 200 tokens, exceeds limit of 100

      // Create buyer token account
      const buyerTokenAccountInfo = await getOrCreateAssociatedTokenAccount(
        provider.connection,
        buyer,
        testTokenMint,
        buyer.publicKey
      );
      const buyerTokenAccount = buyerTokenAccountInfo.address;

      try {
        await program.methods
          .buyTokens(usdcAmount)
          .accounts({
            buyer: buyer.publicKey,
            tokenSale: testTokenSale,
            tokenMint: testTokenMint,
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
        assert.ok(
          errStr.includes("ExceedsMintLimit") || errStr.includes("6004"),
          `Expected ExceedsMintLimit error, got: ${errStr.substring(0, 200)}`
        );
      }
    });

    it("Auto-closes sale when fully sold", async () => {
      // Launch small supply token
      const { tokenMint: smallTokenMint, tokenSale: smallTokenSale, saleTokenAccount: smallSaleTokenAccount } = await setupTestToken(creator);

      await program.methods
        .launchToken(
          "Small Token",
          "SMALL",
          new BN(100000000000), // 100 tokens (9 decimals)
          new BN(1000000),
          new BN(100000000000), // Can buy all at once
          "small"
        )
        .accounts({
          creator: creator.publicKey,
          tokenMint: smallTokenMint,
          tokenSale: smallTokenSale,
          saleTokenAccount: smallSaleTokenAccount,
        })
        .signers([creator])
        .rpc();

      // Create buyer token account
      const buyerSmallTokenAccountInfo = await getOrCreateAssociatedTokenAccount(
        provider.connection,
        buyer,
        smallTokenMint,
        buyer.publicKey
      );
      const buyerSmallTokenAccount = buyerSmallTokenAccountInfo.address;

      // Buy all tokens
      await program.methods
        .buyTokens(new BN(100000000)) // 100 USDC for 100 tokens
        .accounts({
          buyer: buyer.publicKey,
          tokenSale: smallTokenSale,
          tokenMint: smallTokenMint,
          saleTokenAccount: smallSaleTokenAccount,
          buyerTokenAccount: buyerSmallTokenAccount,
          buyerUsdcAccount,
          programAuthority,
          programUsdcAccount,
          ownerUsdcAccount: platformOwnerUsdcAccount,
          creatorUsdcAccount,
          appState,
        })
        .signers([buyer])
        .rpc();

      // Verify sale is closed
      const saleAccount = await program.account.tokenSale.fetch(smallTokenSale);
      assert.isFalse(saleAccount.active);
      assert.equal(saleAccount.tokensSold.toString(), saleAccount.supplyForSale.toString());
    });
  });

  describe("Close Sale", () => {
    let closeTokenMint: anchor.web3.PublicKey;
    let closeTokenSale: anchor.web3.PublicKey;
    let closeSaleTokenAccount: anchor.web3.PublicKey;

    before(async () => {
      const setup = await setupTestToken(creator);
      closeTokenMint = setup.tokenMint;
      closeTokenSale = setup.tokenSale;
      closeSaleTokenAccount = setup.saleTokenAccount;

      await program.methods
        .launchToken(
          "Close Test",
          "CLOSE",
          new BN(1000),
          new BN(1000000),
          new BN(100),
          "close"
        )
        .accounts({
          creator: creator.publicKey,
          tokenMint: closeTokenMint,
          tokenSale: closeTokenSale,
          saleTokenAccount: closeSaleTokenAccount,
        })
        .signers([creator])
        .rpc();
    });

    it("Successfully closes sale and returns tokens", async () => {
      const creatorTokenAccountInfo = await getOrCreateAssociatedTokenAccount(
        provider.connection,
        creator,
        closeTokenMint,
        creator.publicKey
      );
      const creatorTokenAccount = creatorTokenAccountInfo.address;

      const tx = await program.methods
        .closeSale()
        .accounts({
          creator: creator.publicKey,
          tokenSale: closeTokenSale,
          tokenMint: closeTokenMint,
          saleTokenAccount: closeSaleTokenAccount,
          creatorTokenAccount,
        })
        .signers([creator])
        .rpc();

      console.log("Close sale tx:", tx);
      await logGasCost(provider.connection, tx, "Close Sale");

      // Verify sale is closed
      const saleAccount = await program.account.tokenSale.fetch(closeTokenSale);
      assert.isFalse(saleAccount.active);

      // Verify creator received remaining tokens
      const finalCreatorTokenAccount = await getAccount(provider.connection, creatorTokenAccount);
      assert.equal(finalCreatorTokenAccount.amount.toString(), "1000");
    });

    it("Fails when non-creator tries to close", async () => {
      const { tokenMint: anotherTokenMint, tokenSale: anotherTokenSale, saleTokenAccount: anotherSaleTokenAccount } = await setupTestToken(creator);

      await program.methods
        .launchToken(
          "Another",
          "ANOT",
          new BN(1000),
          new BN(1000000),
          new BN(100),
          "another"
        )
        .accounts({
          creator: creator.publicKey,
          tokenMint: anotherTokenMint,
          tokenSale: anotherTokenSale,
          saleTokenAccount: anotherSaleTokenAccount,
        })
        .signers([creator])
        .rpc();

      const buyerTokenAccountInfo = await getOrCreateAssociatedTokenAccount(
        provider.connection,
        buyer,
        anotherTokenMint,
        buyer.publicKey
      );
      const buyerTokenAccount = buyerTokenAccountInfo.address;

      try {
        await program.methods
          .closeSale()
          .accounts({
            creator: buyer.publicKey, // Wrong signer
            tokenSale: anotherTokenSale,
            tokenMint: anotherTokenMint,
            saleTokenAccount: anotherSaleTokenAccount,
            creatorTokenAccount: buyerTokenAccount,
          })
          .signers([buyer])
          .rpc();
        assert.fail("Should have failed");
      } catch (err) {
        const errStr = err.toString();
        // Should fail due to constraint violations (wrong creator, wrong token account, etc.)
        assert.ok(
          errStr.includes("ConstraintAddress") ||
          errStr.includes("2003") ||
          errStr.includes("ConstraintHasOne") ||
          errStr.includes("2001") ||
          errStr.includes("ConstraintToken") ||
          errStr.includes("2014"),
          `Expected constraint error, got: ${errStr.substring(0, 200)}`
        );
      }
    });
  });
});