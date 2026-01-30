use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{
        close_account,
        transfer_checked,
        Mint,
        TokenAccount,
        TokenInterface,
        TransferChecked,
        CloseAccount,
    },
};

use crate::state::Escrow;
use crate::errors::EscrowError;

#[derive(Accounts)]
pub struct Refund<'info> {
    /// Maker: 创建 escrow 的人，退款的发起者
    #[account(mut)]
    pub maker: Signer<'info>,

    /// Escrow PDA：存储交易条款
    /// close = maker 表示关闭后 lamports 返还给 maker
    #[account(
        mut,
        close = maker,
        seeds = [b"escrow", maker.key().as_ref(), escrow.seed.to_le_bytes().as_ref()],
        bump = escrow.bump,
        has_one = maker @ EscrowError::InvalidMaker,
        has_one = mint_a @ EscrowError::InvalidMintA,
    )]
    pub escrow: Box<Account<'info, Escrow>>,

    /// Token A 的 mint
    pub mint_a: Box<InterfaceAccount<'info, Mint>>,

    /// Vault：escrow PDA 持有的 Token A
    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = escrow,
        associated_token::token_program = token_program
    )]
    pub vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Maker 的 Token A ATA（接收退款）
    #[account(
        init_if_needed,
        payer = maker,
        associated_token::mint = mint_a,
        associated_token::authority = maker,
        associated_token::token_program = token_program
    )]
    pub maker_ata_a: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Programs
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

impl<'info> Refund<'info> {

    /// 从 Vault 中把所有 Token A 转回给 Maker，并关闭 Vault
    fn refund_and_close_vault(&mut self) -> Result<()> {
        // Escrow PDA 作为 Vault 的 authority，需要 signer seeds
        let signer_seeds: [&[&[u8]]; 1] = [&[
            b"escrow",
            self.maker.key.as_ref(),
            &self.escrow.seed.to_le_bytes(),
            &[self.escrow.bump],
        ]];

        // Vault -> Maker ATA 转账 Token A
        transfer_checked(
            CpiContext::new_with_signer(
                self.token_program.to_account_info(),
                TransferChecked {
                    from: self.vault.to_account_info(),
                    to: self.maker_ata_a.to_account_info(),
                    mint: self.mint_a.to_account_info(),
                    authority: self.escrow.to_account_info(),
                },
                &signer_seeds,
            ),
            self.vault.amount,        // 全部余额
            self.mint_a.decimals,     // mint 精度
        )?;

        // 关闭 Vault，把 rent lamports 返还给 maker
        close_account(
            CpiContext::new_with_signer(
                self.token_program.to_account_info(),
                CloseAccount {
                    account: self.vault.to_account_info(),
                    authority: self.escrow.to_account_info(),
                    destination: self.maker.to_account_info(),
                },
                &signer_seeds,
            ),
        )?;

        Ok(())
    }
}

pub fn handler(ctx: Context<Refund>) -> Result<()> {
    ctx.accounts.refund_and_close_vault()?;
    Ok(())
}
