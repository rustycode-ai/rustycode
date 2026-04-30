"""Test suite for the MIPS assembler benchmark task."""
import subprocess
import tempfile
import os
import sys
from pathlib import Path

APP_DIR = Path(os.environ.get("APP_DIR", "/app"))


def find_assembler():
    """Find the assembler binary or script."""
    binary = APP_DIR / "mips_asm"
    script = APP_DIR / "mips_asm.py"
    if binary.exists():
        return str(binary)
    if script.exists():
        return f"{sys.executable} {script}"
    raise FileNotFoundError(
        f"No assembler found in {APP_DIR}. "
        "Expected mips_asm or mips_asm.py"
    )


def run_assembler(asm_code: str) -> tuple[str, str, int]:
    """Run the assembler on the given code, return (stdout, stderr, returncode)."""
    asm_cmd = find_assembler()
    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".asm", delete=False
    ) as asm_file:
        asm_file.write(asm_code)
        asm_path = asm_file.name
    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".hex", delete=False
    ) as hex_file:
        hex_path = hex_file.name

    try:
        cmd = f"{asm_cmd} {asm_path} {hex_path}"
        result = subprocess.run(
            cmd, shell=True, capture_output=True, text=True, timeout=30
        )
        output = ""
        if os.path.exists(hex_path):
            output = Path(hex_path).read_text().strip()
        return output, result.stderr, result.returncode
    finally:
        for p in [asm_path, hex_path]:
            if os.path.exists(p):
                os.unlink(p)


def parse_hex_lines(output: str) -> list[str]:
    """Parse hex output into list of 8-char hex strings."""
    return [line.strip().lower() for line in output.strip().split("\n") if line.strip()]


# --- R-type tests ---

def test_add():
    """add $t2, $t0, $t1: opcode=0, rs=8, rt=9, rd=10, shamt=0, funct=0x20"""
    code = ".text\nadd $t2, $t0, $t1\n"
    output, _, rc = run_assembler(code)
    assert rc == 0, f"Exit code {rc}, stderr: {_}"
    lines = parse_hex_lines(output)
    assert lines == ["01095020"], f"Expected ['01095020'], got {lines}"


def test_sub():
    """sub $s0, $t0, $t1: opcode=0, rs=8, rt=9, rd=16, shamt=0, funct=0x22"""
    code = ".text\nsub $s0, $t0, $t1\n"
    output, _, rc = run_assembler(code)
    assert rc == 0
    lines = parse_hex_lines(output)
    assert lines == ["01098022"], f"Expected ['01098022'], got {lines}"


def test_and():
    """and $t3, $t0, $t1"""
    code = ".text\nand $t3, $t0, $t1\n"
    output, _, rc = run_assembler(code)
    assert rc == 0
    lines = parse_hex_lines(output)
    assert lines == ["01095824"], f"Expected ['01095824'], got {lines}"


def test_or():
    """or $t4, $t0, $t1"""
    code = ".text\nor $t4, $t0, $t1\n"
    output, _, rc = run_assembler(code)
    assert rc == 0
    lines = parse_hex_lines(output)
    assert lines == ["01096025"], f"Expected ['01096025'], got {lines}"


def test_slt():
    """slt $t5, $t0, $t1"""
    code = ".text\nslt $t5, $t0, $t1\n"
    output, _, rc = run_assembler(code)
    assert rc == 0
    lines = parse_hex_lines(output)
    assert lines == ["0109682a"], f"Expected ['0109682a'], got {lines}"


# --- I-type tests ---

def test_addi():
    """addi $t0, $t1, 42: opcode=0x08, rs=9, rt=8, imm=0x002a"""
    code = ".text\naddi $t0, $t1, 42\n"
    output, _, rc = run_assembler(code)
    assert rc == 0
    lines = parse_hex_lines(output)
    assert lines == ["2128002a"], f"Expected ['2128002a'], got {lines}"


def test_andi():
    """andi $t0, $t1, 0xff: opcode=0x0c, rs=9, rt=8, imm=0x00ff"""
    code = ".text\nandi $t0, $t1, 255\n"
    output, _, rc = run_assembler(code)
    assert rc == 0
    lines = parse_hex_lines(output)
    assert lines == ["312800ff"], f"Expected ['312800ff'], got {lines}"


def test_ori():
    """ori $t0, $zero, 100: opcode=0x0d, rs=0, rt=8, imm=0x0064"""
    code = ".text\nori $t0, $zero, 100\n"
    output, _, rc = run_assembler(code)
    assert rc == 0
    lines = parse_hex_lines(output)
    assert lines == ["35080064"], f"Expected ['35080064'], got {lines}"


def test_lw():
    """lw $t0, 8($sp): opcode=0x23, rs=29, rt=8, offset=8"""
    code = ".text\nlw $t0, 8($sp)\n"
    output, _, rc = run_assembler(code)
    assert rc == 0
    lines = parse_hex_lines(output)
    assert lines == ["8fa80008"], f"Expected ['8fa80008'], got {lines}"


def test_sw():
    """sw $t0, 0($sp): opcode=0x2b, rs=29, rt=8, offset=0"""
    code = ".text\nsw $t0, 0($sp)\n"
    output, _, rc = run_assembler(code)
    assert rc == 0
    lines = parse_hex_lines(output)
    assert lines == ["afa80000"], f"Expected ['afa80000'], got {lines}"


# --- J-type tests ---

def test_j():
    """j main where main is at word address 0: opcode=0x02, addr=0"""
    code = ".text\nj main\nmain:\nnop\n"
    output, _, rc = run_assembler(code)
    assert rc == 0
    lines = parse_hex_lines(output)
    # j 0 → opcode=0x02, address=0 → 0x08000000
    assert lines[0] == "08000000", f"Expected '08000000', got {lines[0]}"


def test_jal():
    """jal func where func is at word address 3: opcode=0x03, addr=3"""
    code = ".text\njal func\nnop\nnop\nfunc:\nnop\n"
    output, _, rc = run_assembler(code)
    assert rc == 0
    lines = parse_hex_lines(output)
    # jal 3 → 0x0c000003
    assert lines[0] == "0c000003", f"Expected '0c000003', got {lines[0]}"


# --- Pseudo-instruction tests ---

def test_li():
    """li $t0, 10 → ori $t0, $zero, 10"""
    code = ".text\nli $t0, 10\n"
    output, _, rc = run_assembler(code)
    assert rc == 0
    lines = parse_hex_lines(output)
    assert lines == ["3508000a"], f"Expected ['3508000a'], got {lines}"


# --- Label / branch tests ---

def test_beq_forward():
    """beq $t0, $t1, target where target is 3 instructions ahead."""
    code = ".text\nbeq $t0, $t1, target\nnop\nnop\nnop\ntarget:\nnop\n"
    output, _, rc = run_assembler(code)
    assert rc == 0
    lines = parse_hex_lines(output)
    # beq: opcode=0x04, rs=8, rt=9, offset=3 (3 instructions forward from PC+1)
    assert lines[0] == "11090003", f"Expected '11090003', got {lines[0]}"


def test_bne_backward():
    """bne $t0, $t1, loop where loop is 2 instructions back."""
    code = ".text\nloop:\nnop\nnop\nbne $t0, $t1, loop\n"
    output, _, rc = run_assembler(code)
    assert rc == 0
    lines = parse_hex_lines(output)
    # bne at word addr 3, loop at word addr 0: offset = 0 - (3+1) = -4 (signed)
    # -4 as 16-bit signed = 0xfffc
    assert lines[2] == "1509fffc", f"Expected '1509fffc', got {lines[2]}"


# --- Directive tests ---

def test_dot_word():
    """.word 0xdeadbeef should emit that value directly."""
    code = ".text\n.word 0xdeadbeef\n"
    output, _, rc = run_assembler(code)
    assert rc == 0
    lines = parse_hex_lines(output)
    assert lines == ["deadbeef"], f"Expected ['deadbeef'], got {lines}"


def test_dot_text_ignored():
    """.text and .globl should be ignored (produce no output)."""
    code = ".text\n.globl main\nadd $t0, $t1, $t2\n"
    output, _, rc = run_assembler(code)
    assert rc == 0
    lines = parse_hex_lines(output)
    assert len(lines) == 1, f"Expected 1 line, got {len(lines)}: {lines}"


# --- Multi-instruction test ---

def test_full_program():
    """Full program: load two values, add, store, branch, jump."""
    code = """\
.text
main:
    li $t0, 10
    li $t1, 20
    add $t2, $t0, $t1
    sw $t2, 0($sp)
    beq $t2, $zero, end
    addi $t2, $t2, 1
end:
    j main
"""
    output, _, rc = run_assembler(code)
    assert rc == 0, f"Exit code {rc}, stderr: {_}"
    lines = parse_hex_lines(output)
    expected = [
        "3508000a",  # li $t0, 10 → ori $t0, $zero, 10
        "35290014",  # li $t1, 20 → ori $t1, $zero, 20
        "01095020",  # add $t2, $t0, $t1
        "afa20000",  # sw $t2, 0($sp)
        "11400001",  # beq $t2, $zero, end (offset=1)
        "214a0001",  # addi $t2, $t2, 1
        "08000000",  # j main (addr=0)
    ]
    assert lines == expected, f"Expected:\n{expected}\nGot:\n{lines}"


# --- Error handling tests ---

def test_error_on_invalid_instruction():
    """Invalid instruction should cause exit code 1."""
    code = ".text\nxyz $t0, $t1\n"
    _, stderr, rc = run_assembler(code)
    assert rc != 0, f"Expected non-zero exit code, got {rc}"


def test_error_on_undefined_label():
    """Reference to undefined label should cause exit code 1."""
    code = ".text\nj nonexistent\n"
    _, stderr, rc = run_assembler(code)
    assert rc != 0, f"Expected non-zero exit code, got {rc}"


def test_error_on_invalid_register():
    """Invalid register name should cause exit code 1."""
    code = ".text\nadd $t0, $t1, $badreg\n"
    _, stderr, rc = run_assembler(code)
    assert rc != 0, f"Expected non-zero exit code, got {rc}"


# --- Numeric register test ---

def test_numeric_registers():
    """Accept numeric register names like $8 for $t0."""
    code = ".text\nadd $10, $8, $9\n"
    output, _, rc = run_assembler(code)
    assert rc == 0
    lines = parse_hex_lines(output)
    assert lines == ["01095020"], f"Expected ['01095020'], got {lines}"
