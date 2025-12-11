use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer, MintTo, Mint};

declare_id!("54cgw9LKXBQDJ8LapbZxtTRvDdYPFQUAq9BDTrnWd3aW");

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

    /// Token mint account being launched.
    /// Must be initialized with token_sale as mint authority.
    #[account(
        mut,
        mint::authority = token_sale,
    )]
    pub token_mint: Account<'info, Mint>,

    /// Sale token account holding minted tokens.
    #[account(
        mut,
        constraint = sale_token_account.mint == token_mint.key() @ ErrorCode::InvalidMint,
        constraint = sale_token_account.owner == token_sale.key() @ ErrorCode::InvalidTokenAccountOwner,
    )]
    pub sale_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct BuyTokens<'info> {
    #[account(mut)]
    pub buyer: Signer<'info>,

    /// Buyer USDC token account
    #[account(
        mut,
        constraint = buyer_usdc_account.owner == buyer.key() @ ErrorCode::InvalidTokenAccountOwner,
        constraint = buyer_usdc_account.mint == app_state.usdc_mint @ ErrorCode::InvalidMint,
    )]
    pub buyer_usdc_account: Account<'info, TokenAccount>,

    /// Buyer token account to receive sale tokens
    #[account(
        mut,
        constraint = buyer_token_account.owner == buyer.key() @ ErrorCode::InvalidTokenAccountOwner,
        constraint = buyer_token_account.mint == token_mint.key() @ ErrorCode::InvalidMint,
    )]
    pub buyer_token_account: Account<'info, TokenAccount>,

    /// Creator's USDC account to receive funds
    #[account(
        mut,
        constraint = creator_usdc_account.owner == token_sale.creator @ ErrorCode::InvalidTokenAccountOwner,
        constraint = creator_usdc_account.mint == app_state.usdc_mint @ ErrorCode::InvalidMint,
    )]
    pub creator_usdc_account: Account<'info, TokenAccount>,

    /// Platform owner USDC account
    #[account(
        mut,
        constraint = owner_usdc_account.owner == app_state.owner @ ErrorCode::InvalidTokenAccountOwner,
        constraint = owner_usdc_account.mint == app_state.usdc_mint @ ErrorCode::InvalidMint,
    )]
    pub owner_usdc_account: Account<'info, TokenAccount>,

    /// Program USDC vault account
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

    /// Token mint of sale token
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

    /// CHECK: Program authority PDA for signing transfers.
    /// Validated through seeds constraint.
    #[account(seeds = [b"authority"], bump)]
    pub program_authority: AccountInfo<'info>,

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

    /// Token mint account
    #[account(mut)]
    pub token_mint: Account<'info, Mint>,

    /// Sale token account holding remaining tokens
    #[account(
        mut,
        constraint = sale_token_account.mint == token_mint.key() @ ErrorCode::InvalidMint,
        constraint = sale_token_account.owner == token_sale.key() @ ErrorCode::InvalidTokenAccountOwner,
    )]
    pub sale_token_account: Account<'info, TokenAccount>,

    /// Creator token account to receive remaining tokens
    #[account(
        mut,
        constraint = creator_token_account.mint == token_mint.key() @ ErrorCode::InvalidMint,
        constraint = creator_token_account.owner == creator.key() @ ErrorCode::InvalidTokenAccountOwner,
    )]
    pub creator_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

// ==========================
// Program
// ==========================
#[program]
pub mod gasless_launchpad {
    use super::*;

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

    pub fn launch_token(
        ctx: Context<LaunchToken>,
        name: String,
        symbol: String,
        supply: u64,
        price_per_token: u64,
        limit_per_mint: u64,
        metadata_id: String,
    ) -> Result<()> {
        require!(name.len() > 0 && name.len() <= 32, ErrorCode::InvalidNameLength);
        require!(symbol.len() > 0 && symbol.len() <= 10, ErrorCode::InvalidSymbolLength);
        require!(metadata_id.len() <= 100, ErrorCode::MetadataIdTooLong);
        require!(supply > 0, ErrorCode::InvalidSupply);

        let decimals = ctx.accounts.token_mint.decimals;
        let max_supply = 10u64.pow(decimals as u32).checked_mul(1_000_000_000).ok_or(ErrorCode::MathOverflow)?;
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

        // Fix borrow checker
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

    pub fn buy_tokens(ctx: Context<BuyTokens>, usdc_amount: u64) -> Result<()> {
        let sale = &mut ctx.accounts.token_sale;
        let state = &ctx.accounts.app_state;

        require!(sale.active, ErrorCode::SaleNotActive);

        let tokens_to_send: u64;

        if sale.price_per_token > 0 {
            require!(usdc_amount > 0, ErrorCode::AmountMustBePositive);

            // Calculate tokens using proper decimals
            // Formula: tokens = (usdc_amount * 10^token_decimals) / price_per_token
            // USDC has 6 decimals, token has sale.decimals
            let decimals_multiplier = 10u64.pow(sale.decimals as u32);

            tokens_to_send = usdc_amount
                .checked_mul(decimals_multiplier)
                .ok_or(ErrorCode::MathOverflow)?
                .checked_div(sale.price_per_token)
                .ok_or(ErrorCode::MathOverflow)?;

            // Ensure minimum purchase (at least 1 token unit after rounding)
            require!(tokens_to_send > 0, ErrorCode::PurchaseAmountTooSmall);

            if sale.limit_per_mint > 0 {
                require!(tokens_to_send <= sale.limit_per_mint, ErrorCode::ExceedsMintLimit);
            }

            // Calculate and validate fee split
            let fee = usdc_amount
                .checked_mul(state.platform_fee_bps as u64)
                .ok_or(ErrorCode::MathOverflow)?
                .checked_div(10_000)
                .ok_or(ErrorCode::MathOverflow)?;
            let creator_share = usdc_amount.checked_sub(fee).ok_or(ErrorCode::MathOverflow)?;

            // CHECKS-EFFECTS-INTERACTIONS PATTERN
            // Update state BEFORE external calls
            let new_total = sale.tokens_sold.checked_add(tokens_to_send).ok_or(ErrorCode::MathOverflow)?;
            require!(new_total <= sale.supply_for_sale, ErrorCode::InsufficientSupply);
            sale.tokens_sold = new_total;

            if sale.tokens_sold == sale.supply_for_sale {
                sale.active = false;
            }

            // Now perform external calls
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

            // CHECKS-EFFECTS-INTERACTIONS: Update state before external calls
            let new_total = sale.tokens_sold.checked_add(tokens_to_send).ok_or(ErrorCode::MathOverflow)?;
            require!(new_total <= sale.supply_for_sale, ErrorCode::InsufficientSupply);
            sale.tokens_sold = new_total;

            if sale.tokens_sold == sale.supply_for_sale {
                sale.active = false;
            }
        }

        // Transfer tokens to buyer
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
}

// ==========================
// State
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
pub struct TokenBought {
    pub token_mint: Pubkey,
    pub buyer: Pubkey,
    pub usdc_spent: u64,
    pub tokens_received: u64,
}

#[event]
pub struct SaleClosed {
    pub token_mint: Pubkey,
    pub remaining_tokens_returned: u64,
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
}
