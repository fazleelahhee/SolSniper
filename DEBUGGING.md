# SolSniper Debugging Notes

## Issue: IncorrectProgramId on buy instruction

### Error
```
TX found: err={'InstructionError': [5, 'IncorrectProgramId']}
```

### Root Cause
Account ordering in our swap instruction doesn't match the actual PumpSwap AMM program.
The v1 code we based this on is outdated — PumpSwap updated their instruction layout.

### Real PumpSwap Account Ordering (from tx 2Q1F8m4...)
From a working PumpPortal sell tx that calls PumpSwap internally:

```
[ 0] 8jbD5hAg...  — pool (NOT global_config)
[ 1] G88iQevE...  — user (signer)
[ 2] ADyA8hde...  — global_config
[ 3] Dznpng...    — base_mint
[ 4] So1111...    — quote_mint (WSOL)
[ 5] ChVj3Ma9...  — user_base_ata
[ 6] 712ShMjw...  — user_quote_ata
[ 7] 9LijxpuT...  — pool_base_vault
[ 8] FnoWr1wW...  — pool_quote_vault
[ 9] FWsW1xNt...  — protocol_fee_recipient
[10] 7xQYoUjU...  — coin_creator_vault_authority
[11] TokenzQ...   — token_program_2022
[12] Tokenkeg...  — token_program
[13] 11111111...  — system_program
[14] ATokenGP...  — associated_token_program
[15] GS4CU59F...  — event_authority
[16] pAMMBay6...  — program (self-reference)
[17] Em1iSJ9f...  — coin_creator
[18] BV9jJ2qX...  — creator_vault_ata
[19+] additional accounts for volume tracking?
```

### Our (Wrong) Order
```
[ 0] global_config   ← WRONG, should be pool
[ 1] pool            ← WRONG, should be user
[ 2] user
...
```

### TODO
1. Reorder accounts to match real PumpSwap layout
2. event_authority PDA should be derived under PUMPSWAP_PROGRAM, not PUMP_PROGRAM
3. Verify the buy discriminator is still correct
4. Test with real transaction
