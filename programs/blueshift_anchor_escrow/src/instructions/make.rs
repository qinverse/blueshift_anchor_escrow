use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{
        Mint,
        TokenAccount,
        TokenInterface,
        transfer_checked,
        TransferChecked,
    },
};

use crate::state::Escrow;
use crate::errors::EscrowError;

/// Make 指令：
///
/// 1. 创建 Escrow PDA，保存交易条款
/// 2. 创建 Vault（Escrow 拥有的 mint_a ATA）
/// 3. 将 maker 的 Token A 转入 Vault
#[derive(Accounts)]
#[instruction(seed: u64)]
pub struct Make<'info> {
    /// 创建者（maker），决定交易条款并存入 Token A
    #[account(mut)]
    pub maker: Signer<'info>,

    /// Escrow PDA，用于保存交易条款
    ///
    /// PDA seeds:
    /// - "escrow"
    /// - maker 公钥
    /// - 用户提供的 seed（支持同一 maker 创建多个 escrow）
    #[account(
        init,
        payer = maker,
        space = Escrow::INIT_SPACE + Escrow::DISCRIMINATOR.len(),
        seeds = [b"escrow", maker.key().as_ref(), seed.to_le_bytes().as_ref()],
        bump,
    )]
    pub escrow: Account<'info, Escrow>,

    // =======================
    // Token Mint Accounts
    // =======================

    /// Maker 存入的 Token A 的 mint
    ///
    /// 要求：
    /// - 必须由 token_program 拥有（SPL Token 或 Token-2022）
    #[account(
        mint::token_program = token_program
    )]
    pub mint_a: InterfaceAccount<'info, Mint>,

    /// Maker 希望换取的 Token B 的 mint
    ///
    /// 注意：
    /// - Make 阶段不转移 Token B
    /// - 仅记录在 Escrow 中，供 take 阶段使用
    #[account(
        mint::token_program = token_program
    )]
    pub mint_b: InterfaceAccount<'info, Mint>,

    // =======================
    // Token Accounts
    // =======================

    /// Maker 的 Token A 关联代币账户（ATA）
    ///
    /// 用途：
    /// - 从这里转出 Token A 到 Vault
    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = maker,
        associated_token::token_program = token_program
    )]
    pub maker_ata_a: InterfaceAccount<'info, TokenAccount>,

    /// Vault：Escrow 拥有的 Token A ATA
    ///
    /// 特点：
    /// - authority = escrow（PDA）
    /// - Maker 无法单独取回 Token A
    /// - 只能通过 take 或 refund 指令操作
    #[account(
        init,
        payer = maker,
        associated_token::mint = mint_a,
        associated_token::authority = escrow,
        associated_token::token_program = token_program
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,

    // =======================
    // Programs
    // =======================

    /// Associated Token Program（创建 ATA 使用）
    pub associated_token_program: Program<'info, AssociatedToken>,

    /// Token Program（SPL Token 或 Token-2022）
    ///
    /// ⚠️ 要求：
    /// - mint_a 和 mint_b 必须由同一个 token_program 拥有
    pub token_program: Interface<'info, TokenInterface>,

    /// System Program（创建 Escrow PDA）
    pub system_program: Program<'info, System>,
}

impl<'info> Make<'info> {
    /// 初始化 Escrow 账户，保存所有交易条款
    ///
    /// 参数说明：
    /// - seed: PDA 使用的随机种子
    /// - receive: maker 希望收到的 Token B 数量
    /// - bump: Escrow PDA 的 bump，用于后续签名
    pub fn populate_escrow(
        &mut self,
        seed: u64,
        receive: u64,
        bump: u8,
    ) -> Result<()> {
        self.escrow.set_inner(Escrow {
            seed,
            maker: self.maker.key(),
            mint_a: self.mint_a.key(),
            mint_b: self.mint_b.key(),
            receive,
            bump,
        });

        Ok(())
    }

    /// 将 maker 的 Token A 存入 Vault
    ///
    /// 使用 transfer_checked：
    /// - 校验 mint
    /// - 校验 decimals
    /// - 防止精度错误
    pub fn deposit_tokens(&self, amount: u64) -> Result<()> {
        transfer_checked(
            CpiContext::new(
                self.token_program.to_account_info(),
                TransferChecked {
                    from: self.maker_ata_a.to_account_info(),
                    mint: self.mint_a.to_account_info(),
                    to: self.vault.to_account_info(),
                    authority: self.maker.to_account_info(),
                },
            ),
            amount,
            self.mint_a.decimals,
        )?;

        Ok(())
    }
}

/// Make 指令入口
///
/// 参数说明：
/// - seed: 用于区分不同 escrow 的随机数
/// - receive: maker 希望收到的 Token B 数量
/// - amount: maker 存入的 Token A 数量
pub fn handler(
    ctx: Context<Make>,
    seed: u64,
    receive: u64,
    amount: u64,
) -> Result<()> {
    // =======================
    // 参数校验
    // =======================

    // 不允许 0 数量的交易
    require_gt!(receive, 0, EscrowError::InvalidAmount);
    require_gt!(amount, 0, EscrowError::InvalidAmount);

    // （可选但推荐）防止 A 和 B 是同一个 mint
    require_keys_neq!(
        ctx.accounts.mint_a.key(),
        ctx.accounts.mint_b.key(),
        EscrowError::InvalidMintA
    );

    // （可选）提前校验 maker 余额是否足够
    require!(
        ctx.accounts.maker_ata_a.amount >= amount,
        EscrowError::InsufficientFunds
    );

    // =======================
    // 初始化 Escrow
    // =======================
    ctx.accounts
        .populate_escrow(seed, receive, ctx.bumps.escrow)?;

    // =======================
    // 存入 Token A
    // =======================
    ctx.accounts.deposit_tokens(amount)?;

    Ok(())
}

