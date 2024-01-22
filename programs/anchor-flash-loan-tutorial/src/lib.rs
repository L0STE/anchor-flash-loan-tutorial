use anchor_lang::prelude::*;
use anchor_spl::{
    token::{Token, TokenAccount, Mint, Transfer, transfer}, 
    associated_token::AssociatedToken
};
use solana_program::sysvar::instructions::{
        ID as INSTRUCTIONS_SYSVAR_ID,
        load_current_index_checked, 
        load_instruction_at_checked
    };
use anchor_lang::Discriminator;

declare_id!("4z55dAv3ySKCbDKUwG25cUbCyhNcqG7T5ze9aiYr4wpr");

#[program]
pub mod flash_loan {
    use super::*;

    pub fn borrow(ctx: Context<Loan>, borrow_amount: u64) -> Result<()> {
        
        // Make sure we're not sending in an invalid amount that can crash our Protocol
        require!(borrow_amount > 0, ProtocolError::InvalidAmount);

        // Derive the Signer Seeds for the Protocol Account
        let seeds = &[
            b"protocol".as_ref(),
            &[ctx.bumps.protocol]
        ];
        let signer_seeds = &[&seeds[..]];

        // Transfer the funds from the protocol to the borrower
        let transfer_accounts = Transfer {
            from: ctx.accounts.protocol_ata.to_account_info(),
            to: ctx.accounts.borrower_ata.to_account_info(),
            authority: ctx.accounts.protocol.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(ctx.accounts.token_program.to_account_info(), transfer_accounts, signer_seeds);

        transfer(cpi_ctx, borrow_amount)?;

        /* 
        
            Instruction Introspection

            This is the primary means by which we secure our program,
            enforce atomicity while making a great UX for our users.

        */

        let ixs = ctx.accounts.instructions.to_account_info();

        /*

            Disable CPIs
            
            Although we have taken numerous measures to secure this program,
            we can kill CPI to close off even more attack vectors as our 
            current use case doesn't need it.

        */

        let current_index = load_current_index_checked(&ixs)? as usize;
        require_gte!(current_index, 0, ProtocolError::InvalidInstructionIndex);
        let current_ix = load_instruction_at_checked(current_index, &ixs)?;
        require!(crate::check_id(&current_ix.program_id), ProtocolError::ProgramMismatch);

        /* 

            Repay Instruction Check

            Make sure that the last instruction of this transaction is a repay instruction

        */

        // Check how many instruction we have in this transaction
        let instruction_sysvar = ixs.try_borrow_data()?;
        let len = instruction_sysvar.len();

        msg!("Number of instructions: {}", len);
        
        // Ensure we have a finalize ix
        if let Ok(repay_ix) = load_instruction_at_checked(len-1, &ixs) {

            // Instruction checks
            require_keys_eq!(repay_ix.program_id, ID, ProtocolError::InvalidProgram);
            require!(repay_ix.data[0..8].eq(instruction::Repay::DISCRIMINATOR.as_slice()), ProtocolError::InvalidIx);

            // We could check the Wallet and Mint seprately but by checking the ATA we do this automatically
            require_keys_eq!(repay_ix.accounts.get(3).ok_or(ProtocolError::InvalidBorrowerAta)?.pubkey, ctx.accounts.borrower_ata.key(), ProtocolError::InvalidBorrowerAta);
            require_keys_eq!(repay_ix.accounts.get(4).ok_or(ProtocolError::InvalidProtocolAta)?.pubkey, ctx.accounts.protocol_ata.key(), ProtocolError::InvalidProtocolAta);

        } else {
            return Err(ProtocolError::MissingRepayIx.into());
        }

        Ok(())
    }

    pub fn repay(ctx: Context<Loan>) -> Result<()> {
        
        /* 
        
            Instruction Introspection

            This is the primary means by which we secure our program,
            enforce atomicity while making a great UX for our users.

            We already checked for most attack vector so we just want to make
            sure that the in the first place of this transaction there is the 
            borrow instruction.

            Plus we'll take the amount_borrowed from the borrow instruction to
            make sure that we're repaying the right amount.

        */

        let ixs = ctx.accounts.instructions.to_account_info();

        let mut amount_borrowed: u64;

        if let Ok(repay_ix) = load_instruction_at_checked(0, &ixs) {

            // Instruction checks
            require_keys_eq!(repay_ix.program_id, ID, ProtocolError::InvalidProgram);
            require!(repay_ix.data[0..8].eq(instruction::Borrow::DISCRIMINATOR.as_slice()), ProtocolError::InvalidIx);

            // Check the amount borrowed:
            let mut borrowed_data: [u8;8] = [0u8;8];
            borrowed_data.copy_from_slice(&repay_ix.data[8..16]);
            amount_borrowed = u64::from_le_bytes(borrowed_data)

        } else {
            return Err(ProtocolError::MissingBorrowIx.into());
        }

        // Add the fee to the amount borrowed (In our case we hardcoded it to 2%)
        let fee = amount_borrowed.checked_mul(2).unwrap().checked_div(100).ok_or(ProtocolError::Overflow)?;
        amount_borrowed = amount_borrowed.checked_add(fee).ok_or(ProtocolError::Overflow)?;

        // Transfer the funds from the protocol to the borrower
        let transfer_accounts = Transfer {
            from: ctx.accounts.borrower_ata.to_account_info(),
            to: ctx.accounts.protocol_ata.to_account_info(),
            authority: ctx.accounts.borrower.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), transfer_accounts);

        transfer(cpi_ctx, amount_borrowed)?;

        Ok(())

    }
}

#[derive(Accounts)]
#[instruction(borrow_amount: u64)]
pub struct Loan<'info> {
    #[account(mut)]
    pub borrower: Signer<'info>,
    #[account(
        mut,
        seeds = [b"protocol".as_ref()],
        bump,
    )]
    pub protocol: SystemAccount<'info>,

    pub mint: Account<'info, Mint>,
    #[account(
        init_if_needed,
        payer = borrower,
        associated_token::mint = mint,
        associated_token::authority = borrower,
    )]
    pub borrower_ata: Account<'info, TokenAccount>,
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = protocol,
        constraint = protocol_ata.amount > borrow_amount @ProtocolError::NotEnoughFunds,
    )]
    pub protocol_ata: Account<'info, TokenAccount>,

    #[account(address = INSTRUCTIONS_SYSVAR_ID)]
    /// CHECK: InstructionsSysvar account
    instructions: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>
}

#[error_code]
pub enum ProtocolError {
    #[msg("Invalid instruction")]
    InvalidIx,
    #[msg("Invalid instruction index")]
    InvalidInstructionIndex,
    #[msg("Invalid amount")]
    InvalidAmount,
    #[msg("Not enough funds")]
    NotEnoughFunds,
    #[msg("Program Mismatch")]
    ProgramMismatch,
    #[msg("Invalid program")]
    InvalidProgram,
    #[msg("Invalid borrower ATA")]
    InvalidBorrowerAta,
    #[msg("Invalid protocol ATA")]
    InvalidProtocolAta,
    #[msg("Missing repay instruction")]
    MissingRepayIx,
    #[msg("Missing borrow instruction")]
    MissingBorrowIx,
    #[msg("Overflow")]
    Overflow,
}