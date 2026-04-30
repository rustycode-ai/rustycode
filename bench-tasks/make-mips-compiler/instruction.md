# Build a MIPS Assembler

Build a program called `mips_asm` that reads a MIPS assembly file and outputs the encoded machine code as hexadecimal (one 8-character hex value per line, one per instruction).

## Interface

Your program must be runnable as:

```
./mips_asm <input.asm> <output.hex>
```

Or, if using Python:

```
python3 mips_asm.py <input.asm> <output.hex>
```

The program must:
- Read the assembly file from the first argument
- Write hex output to the second argument (one 32-bit instruction per line, lowercase hex, no `0x` prefix)
- Exit with code 0 on success, code 1 on error (with error message to stderr)

## Supported Instructions

### R-type: `opcode(6) | rs(5) | rt(5) | rd(5) | shamt(5) | funct(6)`

All R-type instructions have opcode = `0x00` and shamt = `0`.

| Instruction | funct code |
|-------------|-----------|
| `add`       | `0x20`    |
| `sub`       | `0x22`    |
| `and`       | `0x24`    |
| `or`        | `0x25`    |
| `slt`       | `0x2a`    |

Format: `add $rd, $rs, $rt` (destination first, then two sources)

### I-type: `opcode(6) | rs(5) | rt(5) | immediate(16)`

| Instruction | opcode  |
|-------------|---------|
| `addi`      | `0x08`  |
| `andi`      | `0x0c`  |
| `ori`       | `0x0d`  |
| `lw`        | `0x23`  |
| `sw`        | `0x2b`  |
| `beq`       | `0x04`  |
| `bne`       | `0x05`  |

Format for arithmetic: `addi $rt, $rs, imm`
Format for load/store: `lw $rt, offset($rs)` — encode as `opcode | rs | rt | offset`
Format for branches: `beq $rs, $rt, label` — immediate is the signed word offset from (PC+1) to the label

### J-type: `opcode(6) | address(26)`

| Instruction | opcode  |
|-------------|---------|
| `j`         | `0x02`  |
| `jal`       | `0x03`  |

Format: `j label` — address is the label's word address (byte address >> 2)

### Pseudo-instructions

| Pseudo     | Expansion                    |
|-----------|------------------------------|
| `li $rt, imm` | `ori $rt, $zero, imm`     |

### Directives

- `.text` — marks the start of the text section (ignore)
- `.data` — marks the start of the data section (ignore)
- `.word value` — emit the literal 32-bit value as one word
- `.globl name` — ignore

### Labels

A label is a name followed by a colon (e.g., `main:`). Labels resolve to the word address of the next instruction. The first instruction is at word address 0.

## Register Names

| Name  | Number |
|-------|--------|
| `$zero` | 0    |
| `$at`   | 1    |
| `$v0`-`$v1` | 2-3 |
| `$a0`-`$a3` | 4-7 |
| `$t0`-`$t7` | 8-15 |
| `$s0`-`$s7` | 16-23 |
| `$t8`-`$t9` | 24-25 |
| `$k0`-`$k1` | 26-27 |
| `$gp`   | 28   |
| `$sp`   | 29   |
| `$fp`   | 30   |
| `$ra`   | 31   |

You may also accept numeric register names like `$0` through `$31`.

## Error Handling

- Unknown instruction → print error to stderr, exit 1
- Undefined label → print error to stderr, exit 1
- Invalid register → print error to stderr, exit 1

## Example

Input (`test.asm`):
```
.text
main:
    li $t0, 10
    li $t1, 20
    add $t2, $t0, $t1
    sw $t2, 0($sp)
```

Output (`test.hex`):
```
3508000a
35290014
01095020
afa20000
```

Breakdown:
- `li $t0, 10` → `ori $t0, $zero, 10` → `0x0d|0|8|0x000a` → `3508000a`
- `li $t1, 20` → `ori $t1, $zero, 20` → `0x0d|0|9|0x0014` → `35290014`
- `add $t2, $t0, $t1` → `0|8|9|10|0|0x20` → `01095020`
- `sw $t2, 0($sp)` → `0x2b|29|10|0` → `afa20000`

## Success Criteria

All tests in `/tests/test_mips.py` must pass. The test script will:
1. Create assembly input files
2. Run your assembler
3. Check the hex output matches expected machine code
4. Test error handling on invalid input

Write your assembler to `/app/mips_asm` (or `/app/mips_asm.py` if Python). Make it executable.
