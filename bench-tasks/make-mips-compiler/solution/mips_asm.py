#!/usr/bin/env python3
"""Reference MIPS assembler — oracle solution for make-mips-compiler benchmark."""
import sys
from pathlib import Path

REGISTERS = {
    "$zero": 0, "$at": 1,
    "$v0": 2, "$v1": 3,
    "$a0": 4, "$a1": 5, "$a2": 6, "$a3": 7,
    "$t0": 8, "$t1": 9, "$t2": 10, "$t3": 11,
    "$t4": 12, "$t5": 13, "$t6": 14, "$t7": 15,
    "$s0": 16, "$s1": 17, "$s2": 18, "$s3": 19,
    "$s4": 20, "$s5": 21, "$s6": 22, "$s7": 23,
    "$t8": 24, "$t9": 25,
    "$k0": 26, "$k1": 27,
    "$gp": 28, "$sp": 29, "$fp": 30, "$ra": 31,
}

R_FUNCT = {"add": 0x20, "sub": 0x22, "and": 0x24, "or": 0x25, "slt": 0x2A}
I_OPCODE = {"addi": 0x08, "andi": 0x0C, "ori": 0x0D, "lw": 0x23, "sw": 0x2B, "beq": 0x04, "bne": 0x05}
J_OPCODE = {"j": 0x02, "jal": 0x03}


def reg(name: str) -> int:
    name = name.strip().rstrip(",")
    if name in REGISTERS:
        return REGISTERS[name]
    if name.startswith("$") and name[1:].isdigit():
        n = int(name[1:])
        if 0 <= n <= 31:
            return n
    print(f"Error: unknown register '{name}'", file=sys.stderr)
    sys.exit(1)


def parse_imm(s: str) -> int:
    s = s.strip()
    if s.startswith("0x") or s.startswith("0X"):
        return int(s, 16)
    if s.startswith("-"):
        return int(s)
    return int(s)


def encode_r(rd: int, rs: int, rt: int, funct: int) -> int:
    return (rs << 21) | (rt << 16) | (rd << 11) | funct


def encode_i(opcode: int, rs: int, rt: int, imm: int) -> int:
    return (opcode << 26) | (rs << 21) | (rt << 16) | (imm & 0xFFFF)


def encode_j(opcode: int, addr: int) -> int:
    return (opcode << 26) | (addr & 0x03FFFFFF)


def parse_loadstore(args: list[str]) -> tuple[int, int]:
    """Parse 'lw/sw $rt, offset($rs)' format."""
    rt = reg(args[0])
    mem_part = args[1]
    paren = mem_part.index("(")
    offset = parse_imm(mem_part[:paren])
    rs = reg(mem_part[paren + 1 : mem_part.index(")")])
    return rs, rt, offset


def assemble(source: str) -> list[int]:
    lines = source.split("\n")
    # Pass 1: collect labels and strip directives
    instructions: list[tuple[str, list[str]]] = []
    labels: dict[str, int] = {}

    for raw_line in lines:
        line = raw_line.split("#")[0].strip()
        if not line:
            continue
        if line.endswith(":"):
            label = line[:-1].strip()
            labels[label] = len(instructions)
            continue
        if line.startswith("."):
            if line.startswith(".word"):
                val = parse_imm(line.split()[1])
                instructions.append((".word", [val]))
            # ignore .text, .data, .globl
            continue
        parts = line.replace(",", " ").split()
        mnemonic = parts[0].lower()
        args_str = line[len(parts[0]):]
        args = [a for a in args_str.replace(",", " ").split() if a]
        instructions.append((mnemonic, args))

    # Pass 2: encode
    machine_code: list[int] = []

    for i, (mnemonic, args) in enumerate(instructions):
        if mnemonic == ".word":
            machine_code.append(args[0] & 0xFFFFFFFF)
            continue

        if mnemonic == "li":
            rt = reg(args[0])
            imm = parse_imm(args[1])
            machine_code.append(encode_i(0x0D, 0, rt, imm))
            continue

        if mnemonic in R_FUNCT:
            rd, rs, rt = reg(args[0]), reg(args[1]), reg(args[2])
            machine_code.append(encode_r(rd, rs, rt, R_FUNCT[mnemonic]))

        elif mnemonic in ("lw", "sw"):
            rs, rt, offset = parse_loadstore(args)
            machine_code.append(encode_i(I_OPCODE[mnemonic], rs, rt, offset))

        elif mnemonic in ("addi", "andi", "ori"):
            rt, rs = reg(args[0]), reg(args[1])
            imm = parse_imm(args[2])
            machine_code.append(encode_i(I_OPCODE[mnemonic], rs, rt, imm))

        elif mnemonic in ("beq", "bne"):
            rs, rt = reg(args[0]), reg(args[1])
            target = args[2]
            if target not in labels:
                print(f"Error: undefined label '{target}'", file=sys.stderr)
                sys.exit(1)
            offset = labels[target] - (i + 1)
            machine_code.append(encode_i(I_OPCODE[mnemonic], rs, rt, offset))

        elif mnemonic in J_OPCODE:
            target = args[0]
            if target not in labels:
                print(f"Error: undefined label '{target}'", file=sys.stderr)
                sys.exit(1)
            machine_code.append(encode_j(J_OPCODE[mnemonic], labels[target]))

        elif mnemonic == "nop":
            machine_code.append(0)

        else:
            print(f"Error: unknown instruction '{mnemonic}'", file=sys.stderr)
            sys.exit(1)

    return machine_code


def main():
    if len(sys.argv) != 3:
        print(f"Usage: {sys.argv[0]} <input.asm> <output.hex>", file=sys.stderr)
        sys.exit(1)

    source = Path(sys.argv[1]).read_text()
    code = assemble(source)
    Path(sys.argv[2]).write_text("\n".join(f"{w:08x}" for w in code) + "\n")


if __name__ == "__main__":
    main()
