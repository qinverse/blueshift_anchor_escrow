use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

use crate::state::Escrow;
use crate::errors::EscrowError;

#[derive(Accounts)]
pub struct Take<'info> {
    #[account(mut)]
    pub taker: Signer<'info>,

    #[account(mut)]
    pub maker: SystemAccount<'info>,

    #[account(
        mut,
        seeds = [b"escrow", escrow.maker.as_ref(), escrow.seed.to_le_bytes().as_ref()],
        bump = escrow.bump,
        close = maker
    )]
    pub escrow: Account<'info, Escrow>,

    #[account(
        mut,
        seeds = [b"vault", escrow.key().as_ref()],
        bump
    )]
    pub vault: Account<'info, TokenAccount>,

    #[account(mut)]
    pub taker_ata_a: Account<'info, TokenAccount>,
    #[account(mut)]
    pub taker_ata_b: Account<'info, TokenAccount>,
    #[account(mut)]
    pub maker_ata_b: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

pub fn handler(ctx: Context<Take>) -> Result<()> {
    let escrow = &ctx.accounts.escrow;

    require!(
        ctx.accounts.maker.key() == escrow.maker,
        EscrowError::InvalidMaker
    );

    // taker -> maker (B)
    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.taker_ata_b.to_account_info(),
                to: ctx.accounts.maker_ata_b.to_account_info(),
                authority: ctx.accounts.taker.to_account_info(),
            },
        ),
        escrow.receive,
    )?;

    // vault -> taker (A)
    let seeds = &[
        b"escrow",
        escrow.maker.as_ref(),
        escrow.seed.to_le_bytes().as_ref(),
        &[escrow.bump],
    ];

    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.vault.to_account_info(),
                to: ctx.accounts.taker_ata_a.to_account_info(),
                authority: ctx.accounts.escrow.to_account_info(),
            },
            &[seeds],
        ),
        ctx.accounts.vault.amount,
    )?;

    Ok(())
}
