#![allow(unexpected_cfgs)]
#![allow(deprecated)]

use anchor_lang::{prelude::*, AnchorDeserialize, AnchorSerialize};
use anchor_spl::token::{self, Mint, MintTo, Token, TokenAccount, Transfer};
use light_sdk::{
    account::LightAccount,
    address::v1::derive_address,
    cpi::{v1::CpiAccounts, CpiSigner},
    derive_light_cpi_signer,
    instruction::{account_meta::CompressedAccountMeta, PackedAddressTreeInfo, ValidityProof},
    LightDiscriminator,
};

declare_id!("DuyCbExa6AYgs2J6uqfJKYXPHm8HQn82ju3TYXtsvmqt");

/// CPI signer for Light System Program invocations
pub const LIGHT_CPI_SIGNER: CpiSigner =
    derive_light_cpi_signer!("DuyCbExa6AYgs2J6uqfJKYXPHm8HQn82ju3TYXtsvmqt");

/// Compressed Token Program ID (Light Protocol)
/// cTokenmWW8bLPjZEBAUgYy3zKxQZW6VKi7bqNFEVv3m
pub const COMPRESSED_TOKEN_PROGRAM_ID: &str = "cTokenmWW8bLPjZEBAUgYy3zKxQZW6VKi7bqNFEVv3m";

// ==========================
// Account Structs
// ==========================
#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        init,
        payer = owner,
        space = 8 + AppState::INIT_SPACE,
        seeds = [b"app_state"],
        bump
    )]
    pub app_state: Account<'info, AppState>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateFee<'info> {
    #[account(mut, has_one = owner)]
    pub app_state: Account<'info, AppState>,
    pub owner: Signer<'info>,
}

#[derive(Accounts)]
pub struct LaunchToken<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,

    #[account(
        init,
        payer = creator,
        space = 8 + TokenSale::INIT_SPACE,
        seeds = [b"token_sale", token_mint.key().as_ref()],
        bump
    )]
    pub token_sale: Account<'info, TokenSale>,

    #[account(
        mut,
        mint::authority = token_sale,
    )]
    pub token_mint: Account<'info, Mint>,

    #[account(
        mut,
        constraint = sale_token_account.mint == token_mint.key() @ ErrorCode::InvalidMint,
        constraint = sale_token_account.owner == token_sale.key() @ ErrorCode::InvalidTokenAccountOwner,
    )]
    pub sale_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

/// Launch a token with compressed TokenSale state using ZK compression
/// This creates a compressed sale state and works with compressed tokens
///
/// ARCHITECTURE FOR COMPRESSED TOKENS:
/// 1. This instruction creates a compressed TokenSale state (rent-free via Light Protocol)
/// 2. The token mint should have a token pool registered (done client-side via createTokenPool)
/// 3. Compressed tokens are minted client-side via mintTo from @lightprotocol/compressed-token
/// 4. The sale_authority PDA is used as the mint authority for the compressed token mint
/// 5. Buyers receive compressed tokens (not standard SPL tokens) - ~5000x cheaper
#[derive(Accounts)]
pub struct LaunchTokenCompressed<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,

    /// Token mint - must have a token pool registered for compression
    /// The sale_authority PDA should be the mint authority
    #[account(mut)]
    pub token_mint: Account<'info, Mint>,

    /// CHECK: Sale authority PDA - used as mint authority for compressed tokens
    /// This PDA signs compressed token mint operations
    #[account(
        seeds = [b"sale_authority", token_mint.key().as_ref()],
        bump,
    )]
    pub sale_authority: AccountInfo<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct BuyTokens<'info> {
    #[account(mut)]
    pub buyer: Signer<'info>,

    #[account(
        mut,
        constraint = buyer_usdc_account.owner == buyer.key() @ ErrorCode::InvalidTokenAccountOwner,
        constraint = buyer_usdc_account.mint == app_state.usdc_mint @ ErrorCode::InvalidMint,
    )]
    pub buyer_usdc_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = buyer_token_account.owner == buyer.key() @ ErrorCode::InvalidTokenAccountOwner,
        constraint = buyer_token_account.mint == token_mint.key() @ ErrorCode::InvalidMint,
    )]
    pub buyer_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = creator_usdc_account.owner == token_sale.creator @ ErrorCode::InvalidTokenAccountOwner,
        constraint = creator_usdc_account.mint == app_state.usdc_mint @ ErrorCode::InvalidMint,
    )]
    pub creator_usdc_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = owner_usdc_account.owner == app_state.owner @ ErrorCode::InvalidTokenAccountOwner,
        constraint = owner_usdc_account.mint == app_state.usdc_mint @ ErrorCode::InvalidMint,
    )]
    pub owner_usdc_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = program_usdc_account.owner == program_authority.key() @ ErrorCode::InvalidTokenAccountOwner,
        constraint = program_usdc_account.mint == app_state.usdc_mint @ ErrorCode::InvalidMint,
    )]
    pub program_usdc_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = token_sale.token_mint == token_mint.key() @ ErrorCode::InvalidMint,
    )]
    pub token_sale: Account<'info, TokenSale>,

    #[account(mut)]
    pub token_mint: Account<'info, Mint>,

    #[account(
        mut,
        constraint = sale_token_account.mint == token_mint.key() @ ErrorCode::InvalidMint,
        constraint = sale_token_account.owner == token_sale.key() @ ErrorCode::InvalidTokenAccountOwner,
    )]
    pub sale_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [b"app_state"],
        bump,
    )]
    pub app_state: Account<'info, AppState>,

    /// CHECK: Program authority PDA
    #[account(seeds = [b"authority"], bump)]
    pub program_authority: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
}

/// Buy tokens using compressed TokenSale state
///
/// ARCHITECTURE FOR COMPRESSED TOKEN PURCHASES:
/// 1. This instruction handles USDC payment and updates compressed sale state
/// 2. Compressed tokens are transferred client-side via transfer() from @lightprotocol/compressed-token
/// 3. The sale_authority PDA holds the compressed tokens and signs transfers
/// 4. Buyers receive compressed tokens directly to their wallet (no token account rent needed)
#[derive(Accounts)]
pub struct BuyTokensCompressed<'info> {
    #[account(mut)]
    pub buyer: Signer<'info>,

    /// Buyer's USDC account for payment
    #[account(mut)]
    pub buyer_usdc_account: Account<'info, TokenAccount>,

    /// Creator's USDC account to receive payment
    #[account(mut)]
    pub creator_usdc_account: Account<'info, TokenAccount>,

    /// Platform owner's USDC account for fees
    #[account(mut)]
    pub owner_usdc_account: Account<'info, TokenAccount>,

    /// Program's USDC escrow account
    #[account(mut)]
    pub program_usdc_account: Account<'info, TokenAccount>,

    /// Token mint with compression enabled
    #[account(mut)]
    pub token_mint: Account<'info, Mint>,

    /// App state for fee configuration
    #[account(
        mut,
        seeds = [b"app_state"],
        bump,
    )]
    pub app_state: Account<'info, AppState>,

    /// CHECK: Program authority PDA for USDC transfers
    #[account(seeds = [b"authority"], bump)]
    pub program_authority: AccountInfo<'info>,

    /// CHECK: Sale authority PDA - holds compressed tokens and signs transfers
    #[account(
        seeds = [b"sale_authority", token_mint.key().as_ref()],
        bump,
    )]
    pub sale_authority: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct CloseSale<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,

    #[account(
        mut,
        has_one = creator,
        constraint = token_sale.token_mint == token_mint.key() @ ErrorCode::InvalidMint,
    )]
    pub token_sale: Account<'info, TokenSale>,

    #[account(mut)]
    pub token_mint: Account<'info, Mint>,

    #[account(
        mut,
        constraint = sale_token_account.mint == token_mint.key() @ ErrorCode::InvalidMint,
        constraint = sale_token_account.owner == token_sale.key() @ ErrorCode::InvalidTokenAccountOwner,
    )]
    pub sale_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = creator_token_account.mint == token_mint.key() @ ErrorCode::InvalidMint,
        constraint = creator_token_account.owner == creator.key() @ ErrorCode::InvalidTokenAccountOwner,
    )]
    pub creator_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

/// Close sale using compressed TokenSale state
///
/// ARCHITECTURE FOR CLOSING COMPRESSED TOKEN SALES:
/// 1. This instruction updates the compressed sale state to inactive
/// 2. Remaining compressed tokens are transferred client-side back to creator
/// 3. The sale_authority PDA signs the compressed token transfer
#[derive(Accounts)]
pub struct CloseSaleCompressed<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,

    /// Token mint with compression enabled
    #[account(mut)]
    pub token_mint: Account<'info, Mint>,

    /// CHECK: Sale authority PDA - holds compressed tokens
    #[account(
        seeds = [b"sale_authority", token_mint.key().as_ref()],
        bump,
    )]
    pub sale_authority: AccountInfo<'info>,
}

// ==========================
// Program
// ==========================
#[program]
pub mod gasless_launchpad {
    use super::*;
    use light_sdk::cpi::{
        v1::LightSystemProgramCpi, InvokeLightSystemProgram, LightCpiInstruction,
    };

    pub fn initialize(
        ctx: Context<Initialize>,
        usdc_mint: Pubkey,
        platform_fee_bps: u16,
    ) -> Result<()> {
        require!(platform_fee_bps <= 1000, ErrorCode::InvalidFee);
        let state = &mut ctx.accounts.app_state;
        state.owner = ctx.accounts.owner.key();
        state.usdc_mint = usdc_mint;
        state.platform_fee_bps = platform_fee_bps;
        Ok(())
    }

    pub fn update_fee(ctx: Context<UpdateFee>, new_fee_bps: u16) -> Result<()> {
        require!(new_fee_bps <= 1000, ErrorCode::InvalidFee);
        ctx.accounts.app_state.platform_fee_bps = new_fee_bps;
        Ok(())
    }

    /// Launch a token with standard (non-compressed) TokenSale PDA
    pub fn launch_token(
        ctx: Context<LaunchToken>,
        name: String,
        symbol: String,
        supply: u64,
        price_per_token: u64,
        limit_per_mint: u64,
        metadata_id: String,
    ) -> Result<()> {
        require!(
            name.len() > 0 && name.len() <= 32,
            ErrorCode::InvalidNameLength
        );
        require!(
            symbol.len() > 0 && symbol.len() <= 10,
            ErrorCode::InvalidSymbolLength
        );
        require!(metadata_id.len() <= 100, ErrorCode::MetadataIdTooLong);
        require!(supply > 0, ErrorCode::InvalidSupply);

        let decimals = ctx.accounts.token_mint.decimals;
        let max_supply = 10u64
            .pow(decimals as u32)
            .checked_mul(1_000_000_000)
            .ok_or(ErrorCode::MathOverflow)?;
        require!(supply <= max_supply, ErrorCode::SupplyTooLarge);

        if price_per_token == 0 {
            require!(limit_per_mint > 0, ErrorCode::FreeMintRequiresLimit);
        } else {
            require!(price_per_token >= 1_000, ErrorCode::PriceTooLow);
            if limit_per_mint > 0 {
                require!(limit_per_mint <= supply, ErrorCode::LimitExceedsSupply);
            }
        }

        let sale = &mut ctx.accounts.token_sale;
        sale.creator = ctx.accounts.creator.key();
        sale.token_mint = ctx.accounts.token_mint.key();
        sale.price_per_token = price_per_token;
        sale.supply_for_sale = supply;
        sale.tokens_sold = 0;
        sale.active = true;
        sale.metadata_id = metadata_id.clone();
        sale.limit_per_mint = limit_per_mint;
        sale.decimals = decimals;
        sale.bump = ctx.bumps.token_sale;

        let token_mint_key = ctx.accounts.token_mint.key();
        let seeds = &[b"token_sale", token_mint_key.as_ref(), &[sale.bump]];
        let signer = &[&seeds[..]];

        token::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                MintTo {
                    mint: ctx.accounts.token_mint.to_account_info(),
                    to: ctx.accounts.sale_token_account.to_account_info(),
                    authority: ctx.accounts.token_sale.to_account_info(),
                },
                signer,
            ),
            supply,
        )?;

        emit!(TokenLaunched {
            token_mint: ctx.accounts.token_mint.key(),
            creator: ctx.accounts.creator.key(),
            symbol,
            name,
            price: price_per_token,
            supply,
            limit_per_mint,
            metadata_id,
        });

        Ok(())
    }

    /// Launch a token with compressed TokenSale state using ZK compression
    ///
    /// This creates a compressed TokenSale account (rent-free) to track sale state.
    /// The actual compressed tokens should be:
    /// 1. Created with a token pool via createTokenPool() client-side
    /// 2. Minted as compressed tokens via mintTo() from @lightprotocol/compressed-token
    /// 3. Minted to the sale_authority PDA which will hold the supply for sale
    ///
    /// This instruction ONLY creates the sale state - token minting is done client-side
    /// to properly use compressed tokens (not standard SPL tokens).
    pub fn launch_token_compressed<'info>(
        ctx: Context<'_, '_, '_, 'info, LaunchTokenCompressed<'info>>,
        proof: ValidityProof,
        address_tree_info: PackedAddressTreeInfo,
        output_state_tree_index: u8,
        name: String,
        symbol: String,
        supply: u64,
        price_per_token: u64,
        limit_per_mint: u64,
        metadata_id: String,
    ) -> Result<()> {
        require!(
            name.len() > 0 && name.len() <= 32,
            ErrorCode::InvalidNameLength
        );
        require!(
            symbol.len() > 0 && symbol.len() <= 10,
            ErrorCode::InvalidSymbolLength
        );
        require!(metadata_id.len() <= 100, ErrorCode::MetadataIdTooLong);
        require!(supply > 0, ErrorCode::InvalidSupply);

        let decimals = ctx.accounts.token_mint.decimals;
        let max_supply = 10u64
            .pow(decimals as u32)
            .checked_mul(1_000_000_000)
            .ok_or(ErrorCode::MathOverflow)?;
        require!(supply <= max_supply, ErrorCode::SupplyTooLarge);

        if price_per_token == 0 {
            require!(limit_per_mint > 0, ErrorCode::FreeMintRequiresLimit);
        } else {
            require!(price_per_token >= 1_000, ErrorCode::PriceTooLow);
            if limit_per_mint > 0 {
                require!(limit_per_mint <= supply, ErrorCode::LimitExceedsSupply);
            }
        }

        // Setup Light CPI accounts
        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.creator.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        // Derive compressed account address
        let (address, address_seed) = derive_address(
            &[
                b"compressed_token_sale",
                ctx.accounts.token_mint.key().as_ref(),
            ],
            &address_tree_info
                .get_tree_pubkey(&light_cpi_accounts)
                .map_err(|_| ErrorCode::InvalidAddressTree)?,
            &crate::ID,
        );

        // Create compressed TokenSale account (rent-free via Light Protocol)
        let mut compressed_sale = LightAccount::<CompressedTokenSale>::new_init(
            &crate::ID,
            Some(address),
            output_state_tree_index,
        );

        compressed_sale.creator = ctx.accounts.creator.key();
        compressed_sale.token_mint = ctx.accounts.token_mint.key();
        compressed_sale.price_per_token = price_per_token;
        compressed_sale.supply_for_sale = supply;
        compressed_sale.tokens_sold = 0;
        compressed_sale.active = true;
        compressed_sale.limit_per_mint = limit_per_mint;
        compressed_sale.decimals = decimals;
        // Store the sale authority PDA for client-side token operations
        compressed_sale.sale_authority = ctx.accounts.sale_authority.key();
        compressed_sale.sale_authority_bump = ctx.bumps.sale_authority;

        // Invoke Light System Program to create compressed account
        LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
            .with_light_account(compressed_sale)?
            .with_new_addresses(&[address_tree_info.into_new_address_params_packed(address_seed)])
            .invoke(light_cpi_accounts)?;

        // NOTE: Compressed tokens are minted CLIENT-SIDE using:
        // const mintTx = await mintTo(rpc, payer, mint, saleAuthority, creator, supply);
        // This is done after this instruction succeeds.
        // The sale_authority PDA will hold the compressed tokens for sale.

        emit!(TokenLaunchedCompressed {
            token_mint: ctx.accounts.token_mint.key(),
            creator: ctx.accounts.creator.key(),
            compressed_address: address,
            sale_authority: ctx.accounts.sale_authority.key(),
            symbol,
            name,
            price: price_per_token,
            supply,
            limit_per_mint,
            metadata_id,
        });

        Ok(())
    }

    pub fn buy_tokens(ctx: Context<BuyTokens>, usdc_amount: u64) -> Result<()> {
        let sale = &mut ctx.accounts.token_sale;
        let state = &ctx.accounts.app_state;

        require!(sale.active, ErrorCode::SaleNotActive);

        let tokens_to_send: u64;

        if sale.price_per_token > 0 {
            require!(usdc_amount > 0, ErrorCode::AmountMustBePositive);

            let decimals_multiplier = 10u64.pow(sale.decimals as u32);

            tokens_to_send = usdc_amount
                .checked_mul(decimals_multiplier)
                .ok_or(ErrorCode::MathOverflow)?
                .checked_div(sale.price_per_token)
                .ok_or(ErrorCode::MathOverflow)?;

            require!(tokens_to_send > 0, ErrorCode::PurchaseAmountTooSmall);

            if sale.limit_per_mint > 0 {
                require!(
                    tokens_to_send <= sale.limit_per_mint,
                    ErrorCode::ExceedsMintLimit
                );
            }

            let fee = usdc_amount
                .checked_mul(state.platform_fee_bps as u64)
                .ok_or(ErrorCode::MathOverflow)?
                .checked_div(10_000)
                .ok_or(ErrorCode::MathOverflow)?;
            let creator_share = usdc_amount.checked_sub(fee).ok_or(ErrorCode::MathOverflow)?;

            let new_total = sale
                .tokens_sold
                .checked_add(tokens_to_send)
                .ok_or(ErrorCode::MathOverflow)?;
            require!(new_total <= sale.supply_for_sale, ErrorCode::InsufficientSupply);
            sale.tokens_sold = new_total;

            if sale.tokens_sold == sale.supply_for_sale {
                sale.active = false;
            }

            token::transfer(
                CpiContext::new(
                    ctx.accounts.token_program.to_account_info(),
                    Transfer {
                        from: ctx.accounts.buyer_usdc_account.to_account_info(),
                        to: ctx.accounts.program_usdc_account.to_account_info(),
                        authority: ctx.accounts.buyer.to_account_info(),
                    },
                ),
                usdc_amount,
            )?;

            let auth_seeds = &[b"authority".as_ref(), &[ctx.bumps.program_authority]];
            let auth_signer = &[&auth_seeds[..]];

            token::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info(),
                    Transfer {
                        from: ctx.accounts.program_usdc_account.to_account_info(),
                        to: ctx.accounts.owner_usdc_account.to_account_info(),
                        authority: ctx.accounts.program_authority.to_account_info(),
                    },
                    auth_signer,
                ),
                fee,
            )?;

            token::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info(),
                    Transfer {
                        from: ctx.accounts.program_usdc_account.to_account_info(),
                        to: ctx.accounts.creator_usdc_account.to_account_info(),
                        authority: ctx.accounts.program_authority.to_account_info(),
                    },
                    auth_signer,
                ),
                creator_share,
            )?;
        } else {
            require!(usdc_amount == 0, ErrorCode::FreeMintRequiresZeroPayment);
            require!(sale.limit_per_mint > 0, ErrorCode::LimitPerMintNotSet);
            tokens_to_send = sale.limit_per_mint;

            let new_total = sale
                .tokens_sold
                .checked_add(tokens_to_send)
                .ok_or(ErrorCode::MathOverflow)?;
            require!(new_total <= sale.supply_for_sale, ErrorCode::InsufficientSupply);
            sale.tokens_sold = new_total;

            if sale.tokens_sold == sale.supply_for_sale {
                sale.active = false;
            }
        }

        let token_mint_key = ctx.accounts.token_mint.key();
        let seeds = &[b"token_sale", token_mint_key.as_ref(), &[sale.bump]];
        let signer = &[&seeds[..]];

        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.sale_token_account.to_account_info(),
                    to: ctx.accounts.buyer_token_account.to_account_info(),
                    authority: ctx.accounts.token_sale.to_account_info(),
                },
                signer,
            ),
            tokens_to_send,
        )?;

        emit!(TokenBought {
            token_mint: ctx.accounts.token_mint.key(),
            buyer: ctx.accounts.buyer.key(),
            usdc_spent: usdc_amount,
            tokens_received: tokens_to_send,
        });

        Ok(())
    }

    /// Buy tokens from a compressed TokenSale
    ///
    /// This instruction handles:
    /// 1. USDC payment from buyer to creator (with platform fee)
    /// 2. Updates compressed sale state (tokens_sold, active status)
    ///
    /// The compressed token transfer is done CLIENT-SIDE after this instruction:
    /// const transferTx = await transfer(rpc, payer, mint, tokens_to_send, saleAuthority, buyer.publicKey);
    ///
    /// Returns the tokens_to_send value in the event for client to use.
    pub fn buy_tokens_compressed<'info>(
        ctx: Context<'_, '_, '_, 'info, BuyTokensCompressed<'info>>,
        proof: ValidityProof,
        current_sale: CompressedTokenSale,
        account_meta: CompressedAccountMeta,
        usdc_amount: u64,
    ) -> Result<()> {
        let state = &ctx.accounts.app_state;

        require!(current_sale.active, ErrorCode::SaleNotActive);
        require!(
            current_sale.token_mint == ctx.accounts.token_mint.key(),
            ErrorCode::InvalidMint
        );

        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.buyer.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        let tokens_to_send: u64;
        let mut updated_sale = current_sale.clone();

        if current_sale.price_per_token > 0 {
            require!(usdc_amount > 0, ErrorCode::AmountMustBePositive);

            let decimals_multiplier = 10u64.pow(current_sale.decimals as u32);

            tokens_to_send = usdc_amount
                .checked_mul(decimals_multiplier)
                .ok_or(ErrorCode::MathOverflow)?
                .checked_div(current_sale.price_per_token)
                .ok_or(ErrorCode::MathOverflow)?;

            require!(tokens_to_send > 0, ErrorCode::PurchaseAmountTooSmall);

            if current_sale.limit_per_mint > 0 {
                require!(
                    tokens_to_send <= current_sale.limit_per_mint,
                    ErrorCode::ExceedsMintLimit
                );
            }

            let fee = usdc_amount
                .checked_mul(state.platform_fee_bps as u64)
                .ok_or(ErrorCode::MathOverflow)?
                .checked_div(10_000)
                .ok_or(ErrorCode::MathOverflow)?;
            let creator_share = usdc_amount.checked_sub(fee).ok_or(ErrorCode::MathOverflow)?;

            let new_total = current_sale
                .tokens_sold
                .checked_add(tokens_to_send)
                .ok_or(ErrorCode::MathOverflow)?;
            require!(
                new_total <= current_sale.supply_for_sale,
                ErrorCode::InsufficientSupply
            );

            updated_sale.tokens_sold = new_total;
            if updated_sale.tokens_sold == updated_sale.supply_for_sale {
                updated_sale.active = false;
            }

            // USDC transfers - buyer pays for tokens
            token::transfer(
                CpiContext::new(
                    ctx.accounts.token_program.to_account_info(),
                    Transfer {
                        from: ctx.accounts.buyer_usdc_account.to_account_info(),
                        to: ctx.accounts.program_usdc_account.to_account_info(),
                        authority: ctx.accounts.buyer.to_account_info(),
                    },
                ),
                usdc_amount,
            )?;

            let auth_seeds = &[b"authority".as_ref(), &[ctx.bumps.program_authority]];
            let auth_signer = &[&auth_seeds[..]];

            // Platform fee to owner
            token::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info(),
                    Transfer {
                        from: ctx.accounts.program_usdc_account.to_account_info(),
                        to: ctx.accounts.owner_usdc_account.to_account_info(),
                        authority: ctx.accounts.program_authority.to_account_info(),
                    },
                    auth_signer,
                ),
                fee,
            )?;

            // Creator receives their share
            token::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info(),
                    Transfer {
                        from: ctx.accounts.program_usdc_account.to_account_info(),
                        to: ctx.accounts.creator_usdc_account.to_account_info(),
                        authority: ctx.accounts.program_authority.to_account_info(),
                    },
                    auth_signer,
                ),
                creator_share,
            )?;
        } else {
            // Free mint - no USDC payment
            require!(usdc_amount == 0, ErrorCode::FreeMintRequiresZeroPayment);
            require!(current_sale.limit_per_mint > 0, ErrorCode::LimitPerMintNotSet);
            tokens_to_send = current_sale.limit_per_mint;

            let new_total = current_sale
                .tokens_sold
                .checked_add(tokens_to_send)
                .ok_or(ErrorCode::MathOverflow)?;
            require!(
                new_total <= current_sale.supply_for_sale,
                ErrorCode::InsufficientSupply
            );

            updated_sale.tokens_sold = new_total;
            if updated_sale.tokens_sold == updated_sale.supply_for_sale {
                updated_sale.active = false;
            }
        }

        // Update compressed sale state via Light System Program
        let mut light_account = LightAccount::<CompressedTokenSale>::new_mut(
            &crate::ID,
            &account_meta,
            current_sale.clone(),
        )?;
        light_account.tokens_sold = updated_sale.tokens_sold;
        light_account.active = updated_sale.active;

        LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
            .with_light_account(light_account)?
            .invoke(light_cpi_accounts)?;

        // NOTE: Compressed token transfer is done CLIENT-SIDE after this instruction:
        // const transferTx = await transfer(rpc, saleAuthorityKeypair, mint, tokens_to_send, saleAuthority, buyer.publicKey);
        // The sale_authority PDA needs to sign this transfer.
        // For this, the client needs to derive the PDA and use approveAndMintTo or similar pattern.

        emit!(TokenBoughtCompressed {
            token_mint: ctx.accounts.token_mint.key(),
            buyer: ctx.accounts.buyer.key(),
            usdc_spent: usdc_amount,
            tokens_received: tokens_to_send,
            sale_authority: current_sale.sale_authority,
            sale_authority_bump: current_sale.sale_authority_bump,
        });

        Ok(())
    }

    pub fn close_sale(ctx: Context<CloseSale>) -> Result<()> {
        let sale = &mut ctx.accounts.token_sale;
        require!(sale.active, ErrorCode::AlreadyClosed);
        sale.active = false;

        let remaining = ctx.accounts.sale_token_account.amount;

        if remaining > 0 {
            let token_mint_key = ctx.accounts.token_mint.key();
            let seeds = &[b"token_sale", token_mint_key.as_ref(), &[sale.bump]];
            let signer = &[&seeds[..]];

            token::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info(),
                    Transfer {
                        from: ctx.accounts.sale_token_account.to_account_info(),
                        to: ctx.accounts.creator_token_account.to_account_info(),
                        authority: ctx.accounts.token_sale.to_account_info(),
                    },
                    signer,
                ),
                remaining,
            )?;
        }

        emit!(SaleClosed {
            token_mint: ctx.accounts.token_mint.key(),
            remaining_tokens_returned: remaining,
        });

        Ok(())
    }

    /// Close a compressed TokenSale and return remaining tokens
    ///
    /// This instruction:
    /// 1. Verifies creator authorization
    /// 2. Updates compressed sale state to inactive
    ///
    /// The compressed token transfer (returning remaining tokens to creator) is done CLIENT-SIDE:
    /// const remainingTokens = supply_for_sale - tokens_sold;
    /// const transferTx = await transfer(rpc, saleAuthorityKeypair, mint, remainingTokens, saleAuthority, creator.publicKey);
    pub fn close_sale_compressed<'info>(
        ctx: Context<'_, '_, '_, 'info, CloseSaleCompressed<'info>>,
        proof: ValidityProof,
        current_sale: CompressedTokenSale,
        account_meta: CompressedAccountMeta,
    ) -> Result<()> {
        require!(current_sale.active, ErrorCode::AlreadyClosed);
        require!(
            current_sale.creator == ctx.accounts.creator.key(),
            ErrorCode::Unauthorized
        );
        require!(
            current_sale.token_mint == ctx.accounts.token_mint.key(),
            ErrorCode::InvalidMint
        );

        let light_cpi_accounts = CpiAccounts::new(
            ctx.accounts.creator.as_ref(),
            ctx.remaining_accounts,
            crate::LIGHT_CPI_SIGNER,
        );

        // Calculate remaining tokens (for the event)
        let remaining_tokens = current_sale
            .supply_for_sale
            .checked_sub(current_sale.tokens_sold)
            .ok_or(ErrorCode::MathOverflow)?;

        // Update compressed account to mark as closed
        let mut light_account = LightAccount::<CompressedTokenSale>::new_mut(
            &crate::ID,
            &account_meta,
            current_sale.clone(),
        )?;
        light_account.active = false;

        LightSystemProgramCpi::new_cpi(LIGHT_CPI_SIGNER, proof)
            .with_light_account(light_account)?
            .invoke(light_cpi_accounts)?;

        // NOTE: Compressed token transfer (returning remaining tokens) is done CLIENT-SIDE:
        // const transferTx = await transfer(rpc, saleAuthorityKeypair, mint, remainingTokens, saleAuthority, creator.publicKey);

        emit!(SaleClosedCompressed {
            token_mint: ctx.accounts.token_mint.key(),
            remaining_tokens_returned: remaining_tokens,
            sale_authority: current_sale.sale_authority,
            sale_authority_bump: current_sale.sale_authority_bump,
        });

        Ok(())
    }
}

// ==========================
// State (Regular PDA)
// ==========================
#[account]
#[derive(InitSpace)]
pub struct AppState {
    pub owner: Pubkey,
    pub usdc_mint: Pubkey,
    pub platform_fee_bps: u16,
}

#[account]
#[derive(InitSpace)]
pub struct TokenSale {
    pub creator: Pubkey,
    pub token_mint: Pubkey,
    pub price_per_token: u64,
    pub supply_for_sale: u64,
    pub tokens_sold: u64,
    pub active: bool,
    #[max_len(100)]
    pub metadata_id: String,
    pub limit_per_mint: u64,
    pub decimals: u8,
    pub bump: u8,
}

// ==========================
// Compressed State (ZK Compression)
// ==========================
/// Compressed TokenSale account - stored in Light Protocol merkle tree
/// No rent required - only pays for compression proof (~0.00001 SOL vs ~0.002 SOL)
///
/// This state tracks the sale configuration. The actual compressed tokens are:
/// - Minted via @lightprotocol/compressed-token SDK (client-side)
/// - Held by the sale_authority PDA
/// - Transferred to buyers via compressed token transfers (client-side)
#[event]
#[derive(Clone, Debug, Default, LightDiscriminator)]
pub struct CompressedTokenSale {
    /// Creator who launched the sale
    pub creator: Pubkey,
    /// Token mint address (must have token pool registered)
    pub token_mint: Pubkey,
    /// Price per token in USDC (with 6 decimals), 0 for free mints
    pub price_per_token: u64,
    /// Total supply available for sale
    pub supply_for_sale: u64,
    /// Number of tokens already sold
    pub tokens_sold: u64,
    /// Whether the sale is active
    pub active: bool,
    /// Maximum tokens per purchase (0 = unlimited for paid, required for free)
    pub limit_per_mint: u64,
    /// Token decimals
    pub decimals: u8,
    /// Sale authority PDA that holds the compressed tokens
    pub sale_authority: Pubkey,
    /// Bump seed for the sale_authority PDA
    pub sale_authority_bump: u8,
}

// ==========================
// Events
// ==========================
#[event]
pub struct TokenLaunched {
    pub token_mint: Pubkey,
    pub creator: Pubkey,
    pub symbol: String,
    pub name: String,
    pub price: u64,
    pub supply: u64,
    pub limit_per_mint: u64,
    pub metadata_id: String,
}

#[event]
pub struct TokenLaunchedCompressed {
    pub token_mint: Pubkey,
    pub creator: Pubkey,
    pub compressed_address: [u8; 32],
    /// Sale authority PDA - client should mint compressed tokens to this address
    pub sale_authority: Pubkey,
    pub symbol: String,
    pub name: String,
    pub price: u64,
    pub supply: u64,
    pub limit_per_mint: u64,
    pub metadata_id: String,
}

#[event]
pub struct TokenBought {
    pub token_mint: Pubkey,
    pub buyer: Pubkey,
    pub usdc_spent: u64,
    pub tokens_received: u64,
}

#[event]
pub struct TokenBoughtCompressed {
    pub token_mint: Pubkey,
    pub buyer: Pubkey,
    pub usdc_spent: u64,
    /// Number of compressed tokens to transfer to buyer (client-side)
    pub tokens_received: u64,
    /// Sale authority PDA holding the compressed tokens
    pub sale_authority: Pubkey,
    /// Bump for the sale authority PDA
    pub sale_authority_bump: u8,
}

#[event]
pub struct SaleClosed {
    pub token_mint: Pubkey,
    pub remaining_tokens_returned: u64,
}

#[event]
pub struct SaleClosedCompressed {
    pub token_mint: Pubkey,
    /// Number of remaining compressed tokens to return to creator (client-side)
    pub remaining_tokens_returned: u64,
    /// Sale authority PDA holding the remaining compressed tokens
    pub sale_authority: Pubkey,
    /// Bump for the sale authority PDA
    pub sale_authority_bump: u8,
}

// ==========================
// Errors
// ==========================
#[error_code]
pub enum ErrorCode {
    #[msg("Name must be between 1 and 32 characters")]
    InvalidNameLength,
    #[msg("Symbol must be between 1 and 10 characters")]
    InvalidSymbolLength,
    #[msg("Supply must be greater than 0")]
    InvalidSupply,
    #[msg("Supply exceeds maximum allowed for this token's decimals")]
    SupplyTooLarge,
    #[msg("Free mints require a positive limit_per_mint")]
    FreeMintRequiresLimit,
    #[msg("Price must be at least 1000 (0.001 USDC)")]
    PriceTooLow,
    #[msg("Limit per mint cannot exceed total supply")]
    LimitExceedsSupply,
    #[msg("Sale is not active")]
    SaleNotActive,
    #[msg("Amount must be positive for paid sales")]
    AmountMustBePositive,
    #[msg("Purchase amount exceeds mint limit")]
    ExceedsMintLimit,
    #[msg("Free mints must send 0 USDC")]
    FreeMintRequiresZeroPayment,
    #[msg("Limit per mint must be set for this sale")]
    LimitPerMintNotSet,
    #[msg("Insufficient tokens remaining for this purchase")]
    InsufficientSupply,
    #[msg("Sale is already closed")]
    AlreadyClosed,
    #[msg("Mathematical operation overflow")]
    MathOverflow,
    #[msg("Platform fee must be <= 1000 basis points (10%)")]
    InvalidFee,
    #[msg("Invalid token mint")]
    InvalidMint,
    #[msg("Invalid token account owner")]
    InvalidTokenAccountOwner,
    #[msg("Purchase amount too small - results in 0 tokens")]
    PurchaseAmountTooSmall,
    #[msg("Metadata ID must be <= 100 characters")]
    MetadataIdTooLong,
    #[msg("Invalid address tree")]
    InvalidAddressTree,
    #[msg("Unauthorized")]
    Unauthorized,
}
