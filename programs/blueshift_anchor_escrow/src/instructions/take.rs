use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{
        transfer_checked,
        close_account,
        Mint,
        TokenAccount,
        TokenInterface,
        TransferChecked,
        CloseAccount,
    },
};

use crate::state::Escrow;
use crate::errors::EscrowError;

/// Take 指令：
/// - taker 用 Token B 换取 Vault 中的 Token A
/// - Token B：taker -> maker
/// - Token A：vault -> taker
/// - 关闭 vault
/// - 关闭 escrow（lamports 返还给 maker）
#[derive(Accounts)]
pub struct Take<'info> {
    /// 接受报价的用户（支付 Token B）
    #[account(mut)]
    pub taker: Signer<'info>,

    /// 创建 escrow 的用户（接收 Token B + lamports）
    #[account(mut)]
    pub maker: SystemAccount<'info>,

    /// Escrow 状态账户
    /// - 使用 PDA 校验
    /// - 执行完成后关闭，lamports 返还给 maker
    #[account(
        mut,
        close = maker,
        seeds = [b"escrow", maker.key().as_ref(), escrow.seed.to_le_bytes().as_ref()],
        bump = escrow.bump,
        has_one = maker @ EscrowError::InvalidMaker,
        has_one = mint_a @ EscrowError::InvalidMintA,
        has_one = mint_b @ EscrowError::InvalidMintB,
    )]
    pub escrow: Box<Account<'info, Escrow>>,

    /// ===== Token Mints =====

    /// Token A（从 vault 转给 taker）
    pub mint_a: Box<InterfaceAccount<'info, Mint>>,

    /// Token B（从 taker 转给 maker）
    pub mint_b: Box<InterfaceAccount<'info, Mint>>,

    /// ===== Token Accounts =====

    /// Vault：escrow 持有的 Token A
    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = escrow,
        associated_token::token_program = token_program
    )]
    pub vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Taker 的 Token A ATA（接收 vault 的 Token A）
    #[account(
        init_if_needed,
        payer = taker,
        associated_token::mint = mint_a,
        associated_token::authority = taker,
        associated_token::token_program = token_program
    )]
    pub taker_ata_a: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Taker 的 Token B ATA（支付给 maker）
    #[account(
        mut,
        associated_token::mint = mint_b,
        associated_token::authority = taker,
        associated_token::token_program = token_program
    )]
    pub taker_ata_b: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Maker 的 Token B ATA（接收 taker 的 Token B）
    #[account(
        init_if_needed,
        payer = taker,
        associated_token::mint = mint_b,
        associated_token::authority = maker,
        associated_token::token_program = token_program
    )]
    pub maker_ata_b: Box<InterfaceAccount<'info, TokenAccount>>,

    /// ===== Programs =====

    /// 创建 ATA 所需
    pub associated_token_program: Program<'info, AssociatedToken>,

    /// Token CPI（SPL Token / Token-2022）
    pub token_program: Interface<'info, TokenInterface>,

    /// System Program（用于账户关闭返 lamports）
    pub system_program: Program<'info, System>,
}

impl<'info> Take<'info> {
    /// 将 Token B 从 taker 转给 maker
    fn transfer_to_maker(&mut self) -> Result<()> {
        transfer_checked(
            CpiContext::new(
                self.token_program.to_account_info(),
                TransferChecked {
                    from: self.taker_ata_b.to_account_info(),
                    to: self.maker_ata_b.to_account_info(),
                    mint: self.mint_b.to_account_info(),
                    authority: self.taker.to_account_info(),
                },
            ),
            self.escrow.receive,      // maker 期望收到的 Token B 数量
            self.mint_b.decimals,     // 精度校验
        )?;

        Ok(())
    }

    /// 从 vault 提取 Token A 给 taker，并关闭 vault
    fn withdraw_and_close_vault(&mut self) -> Result<()> {
        // escrow PDA 作为 signer
        let signer_seeds: [&[&[u8]]; 1] = [&[
            b"escrow",
            self.maker.key.as_ref(),
            &self.escrow.seed.to_le_bytes(),
            &[self.escrow.bump],
        ]];

        // 1️⃣ Vault -> Taker（Token A）
        transfer_checked(
            CpiContext::new_with_signer(
                self.token_program.to_account_info(),
                TransferChecked {
                    from: self.vault.to_account_info(),
                    to: self.taker_ata_a.to_account_info(),
                    mint: self.mint_a.to_account_info(),
                    authority: self.escrow.to_account_info(),
                },
                &signer_seeds,
            ),
            self.vault.amount,        // vault 中全部 Token A
            self.mint_a.decimals,
        )?;

        // 2️⃣ 关闭 vault，lamports 返还给 maker
        close_account(
            CpiContext::new_with_signer(
                self.token_program.to_account_info(),
                CloseAccount {
                    account: self.vault.to_account_info(),
                    authority: self.escrow.to_account_info(),
                    destination: self.maker.to_account_info(),
                },
                &signer_seeds,
            )
        )?;

        Ok(())
    }
}

/// Take 指令入口
pub fn handler(ctx: Context<Take>) -> Result<()> {
    // 1️⃣ taker -> maker（Token B）
    ctx.accounts.transfer_to_maker()?;

    // 2️⃣ vault -> taker（Token A）+ 关闭 vault
    ctx.accounts.withdraw_and_close_vault()?;

    // escrow 会因 close = maker 自动关闭
    Ok(())
}
