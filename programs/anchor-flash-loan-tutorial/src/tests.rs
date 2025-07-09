/* Tests */

use anchor_lang::Discriminator;
use anchor_spl::associated_token::get_associated_token_address;
use mollusk_svm::{program::keyed_account_for_system_program, result::Check, Mollusk};
use solana_account::{Account, AccountSharedData};
use solana_instruction::{AccountMeta, Instruction};
use solana_program::{system_program, sysvar::instructions::{construct_instructions_data, store_current_index, BorrowedAccountMeta, BorrowedInstruction}};
use solana_pubkey::Pubkey;

#[test]
fn create_class() {
    let borrower = Pubkey::new_from_array([2u8; 32]);
    let (protocol, _) = Pubkey::find_program_address(
        &[b"protocol"],
        &crate::ID,
    );
    let mint = Pubkey::new_from_array([3u8; 32]);
    let borrower_ata = get_associated_token_address(&borrower, &mint);
    let protocol_ata = get_associated_token_address(&protocol, &mint);
    let (system_program, system_program_data) = keyed_account_for_system_program();
    let (associated_token_program, associated_token_program_data) =
        mollusk_svm_programs_token::associated_token::keyed_account();
    let (token_program, token_program_data) = mollusk_svm_programs_token::token::keyed_account();

    let borrow_instruction = Instruction::new_with_bytes(
        crate::ID,
        &[
            &crate::instruction::Borrow::DISCRIMINATOR,
            &100_000_000u64.to_le_bytes()[..],
        ]
        .concat(),
        vec![
            AccountMeta::new(borrower, true),
            AccountMeta::new(protocol, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new_readonly(borrower_ata, false),
            AccountMeta::new(protocol_ata, false),
            AccountMeta::new(solana_program::sysvar::instructions::ID, false),
            AccountMeta::new_readonly(associated_token_program, false),
            AccountMeta::new_readonly(token_program, false),
            AccountMeta::new_readonly(system_program, false),
        ],
    );

    let repay_instruction = Instruction::new_with_bytes(
        crate::ID,
        &[
            crate::instruction::Repay::DISCRIMINATOR,
        ]
        .concat(),
        vec![
            AccountMeta::new(borrower, true),
            AccountMeta::new(protocol, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new_readonly(borrower_ata, false),
            AccountMeta::new(protocol_ata, false),
            AccountMeta::new(solana_program::sysvar::instructions::ID, false),
            AccountMeta::new_readonly(associated_token_program, false),
            AccountMeta::new_readonly(token_program, false),
            AccountMeta::new_readonly(system_program, false),
        ],
    );

    let mut mollusk = Mollusk::new(
        &crate::ID,
        "../target/deploy/anchor_flash_loan_tutorial",
    );

    let (instruction_sysvar_pubkey, instruction_sysvar_account) = get_account_instructions_sysvar(&mut mollusk, &[borrow_instruction, repay_instruction]);

    mollusk_svm_programs_token::associated_token::add_program(&mut mollusk);
    mollusk_svm_programs_token::token2022::add_program(&mut mollusk);

    let result = mollusk.process_and_validate_instruction_chain(
        &[
            (&borrow_instruction, &[Check::success()]),
            (&repay_instruction, &[Check::success(), Check::account(&borrower_ata).data(&borrower_ata_data.data).build()]),
        ],
        &[
            (borrower, Account::new(1_000_000_000, 0, &system_program::ID)),
            (protocol, Account::new(0, 0, &system_program::ID)),
            (mint, AccountSharedData::new(0, 0, &system_program::ID)),
            (borrower_ata, AccountSharedData::new(0, 0, &system_program::ID)),
            (protocol_ata, AccountSharedData::new(0, 0, &system_program::ID)),
            (instruction_sysvar_pubkey, instruction_sysvar_account.into()),
            (system_program, system_program_data),
            (token_program, token_program_data),
            (associated_token_program, associated_token_program_data),
        ]
    );



}

fn get_account_instructions_sysvar(mollusk: &mut Mollusk, instructions: &[Instruction]) -> (Pubkey, AccountSharedData) {
    // Construct the instructions data from all instructions
    let mut data = construct_instructions_data(
        instructions.iter().map(|instruction| {
            BorrowedInstruction {
                program_id: &instruction.program_id,
                accounts: instruction.accounts
                    .iter()
                    .map(|meta| BorrowedAccountMeta {
                        pubkey: &meta.pubkey,
                        is_signer: meta.is_signer,
                        is_writable: meta.is_writable,
                    })
                    .collect(),
                data: &instruction.data,
            }
        }).collect::<Vec<_>>().as_slice()
    );

    // Find which instruction contains the sysvar account and at what position
    if let Some((ix_index, _)) = instructions.iter().enumerate()
        .find(|(_, instruction)| {
            instruction.accounts.iter()
                .any(|meta| meta.pubkey == solana_program::sysvar::instructions::ID)
        }) 
    {
        store_current_index(&mut data, ix_index as u16);
    }

    // Set the account data
    let mut instruction_sysvar_account = AccountSharedData::new(
        mollusk
            .sysvars
            .rent
            .minimum_balance(data.len()),
        data.len(),
        &system_program::ID
    );
    instruction_sysvar_account.set_data_from_slice(&data);

    (solana_program::sysvar::instructions::ID, instruction_sysvar_account)
}