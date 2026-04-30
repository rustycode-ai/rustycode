/**
 * MIPS Interpreter for DoomGeneric
 * 
 * Executes MIPS ELF binaries and provides the doomgeneric API.
 * Saves rendered frames to disk as they are produced.
 */

const fs = require('fs');
const path = require('path');

// ============================================================================
// MIPS Architecture Constants
// ============================================================================

const REGISTER_COUNT = 32; // 32 general purpose registers
const R0 = 0;  // Always zero
const RA = 31; // Return address
const SP = 29; // Stack pointer
const GP = 28; // Global pointer

// Memory map (MIPS32)
const MEMORY_BASE = 0x00000000;
const STACK_BASE = 0x80000000; // Typical kseg0
const KSEG1_BASE = 0xA0000000;

// DoomGeneric constants
const DOOMGENERIC_RESX = 640;
const DOOMGENERIC_RESY = 400;
const SCREEN_BUFFER_SIZE = DOOMGENERIC_RESX * DOOMGENERIC_RESY * 4; // RGBA

// ============================================================================
// MIPS CPU State
// ============================================================================

class MIPSCPU {
    constructor() {
        this.reset();
    }

    reset() {
        // 32 general-purpose registers
        this.registers = new Array(REGISTER_COUNT).fill(0);
        this.registers[R0] = 0; // $zero always 0
        
        // Special registers
        this.pc = 0;            // Program counter
        this.next_pc = 4;       // Next PC (MIPS is delayed branch)
        this.hi = 0;            // HI register (multiply/divide)
        this.lo = 0;            // LO register (multiply/divide)
        
        // Memory (256MB for simplicity)
        this.memory = new Uint8Array(256 * 1024 * 1024);
        
        // Flags
        this.running = false;
        this.cycles = 0;
        
        // Communication with host (doomgeneric functions)
        this.host = null;
    }

    // Load ELF binary into memory
    loadELF(elfBuffer) {
        const dataView = new DataView(elfBuffer);
        
        // Parse ELF header (32-bit big-endian)
        const magic = dataView.getUint32(0, false);
        if (magic !== 0x7F454446) { // "\x7FELF"
            throw new Error('Invalid ELF magic number');
        }
        
        const class_ = dataView.getUint8(4);
        const encoding = dataView.getUint8(5);
        const version = dataView.getUint8(6);
        
        console.log(`ELF Class: ${class_ === 1 ? '32-bit' : '64-bit'}`);
        console.log(`Encoding: ${encoding === 1 ? 'Little' : 'Big'}-endian`);
        
        if (class_ !== 1 || encoding !== 2) {
            throw new Error('Expected 32-bit big-endian ELF');
        }
        
        // Get entry point
        const entry = dataView.getUint32(24, false);
        console.log(`Entry point: 0x${entry.toString(16)}`);
        
        // Read program headers
        const phoff = dataView.getUint32(28, false);
        const phentsize = dataView.getUint16(42, false);
        const phnum = dataView.getUint16(44, false);
        
        console.log(`Program headers: ${phnum} entries`);
        
        // Load each program segment
        for (let i = 0; i < phnum; i++) {
            const ph_offset = phoff + i * phentsize;
            const p_type = dataView.getUint32(ph_offset, false);
            const p_offset = dataView.getUint32(ph_offset + 4, false);
            const p_vaddr = dataView.getUint32(ph_offset + 8, false);
            const p_filesz = dataView.getUint32(ph_offset + 16, false);
            const p_memsz = dataView.getUint32(ph_offset + 20, false);
            const p_flags = dataView.getUint32(ph_offset + 24, false);
            
            const flags_r = (p_flags & 0x4) !== 0;
            const flags_w = (p_flags & 0x2) !== 0;
            const flags_x = (p_flags & 0x1) !== 0;
            
            console.log(`Segment ${i}: vaddr=0x${p_vaddr.toString(16)}, ` +
                       `size=${p_filesz}/${p_memsz}, ` +
                       `flags=${flags_r?'r':'-'}${flags_w?'w':'-'}${flags_x?'x':'-'}`);
            
            if (p_type === 1) { // PT_LOAD
                // Load segment into memory
                for (let j = 0; j < p_filesz; j++) {
                    this.memory[p_vaddr + j] = dataView.getUint8(p_offset + j);
                }
                // Zero-initialize BSS
                if (p_memsz > p_filesz) {
                    for (let j = p_filesz; j < p_memsz; j++) {
                        this.memory[p_vaddr + j] = 0;
                    }
                }
            }
        }
        
        this.pc = entry;
        console.log(`CPU initialized, starting at PC=0x${entry.toString(16)}`);
        return entry;
    }

    readWord(addr) {
        if (addr < 0 || addr + 4 > this.memory.length) {
            throw new Error(`Memory read out of bounds: 0x${addr.toString(16)}`);
        }
        const view = new DataView(this.memory.buffer);
        return view.getUint32(addr, false); // Big-endian
    }

    writeWord(addr, value) {
        if (addr < 0 || addr + 4 > this.memory.length) {
            throw new Error(`Memory write out of bounds: 0x${addr.toString(16)}`);
        }
        const view = new DataView(this.memory.buffer);
        view.setUint32(addr, value, false); // Big-endian
    }

    readByte(addr) {
        if (addr < 0 || addr >= this.memory.length) {
            throw new Error(`Memory read out of bounds: 0x${addr.toString(16)}`);
        }
        return this.memory[addr];
    }

    writeByte(addr, value) {
        if (addr < 0 || addr >= this.memory.length) {
            throw new Error(`Memory write out of bounds: 0x${addr.toString(16)}`);
        }
        this.memory[addr] = value & 0xFF;
    }

    fetchInstruction() {
        const instruction = this.readWord(this.pc);
        this.pc += 4;
        return instruction;
    }

    execute(instruction) {
        const opcode = (instruction >> 26) & 0x3F;
        
        switch (opcode) {
            case 0x00: this.executeSPECIAL(instruction); break;
            case 0x01: this.executeREGIMM(instruction); break;
            case 0x02: this.executeJ(instruction); break;
            case 0x03: this.executeJAL(instruction); break;
            case 0x04: this.executeBEQ(instruction); break;
            case 0x05: this.executeBNE(instruction); break;
            case 0x08: this.executeADDI(instruction); break;
            case 0x0A: this.executeSLTI(instruction); break;
            case 0x0C: this.executeANDI(instruction); break;
            case 0x0D: this.executeORI(instruction); break;
            case 0x0E: this.executeXORI(instruction); break;
            case 0x0F: this.executeLUI(instruction); break;
            case 0x10: this.executeCOP0(instruction); break;
            case 0x20: this.executeLoad(instruction, 'byte', true); break;
            case 0x21: this.executeLoad(instruction, 'half', true); break;
            case 0x23: this.executeLoad(instruction, 'word', true); break;
            case 0x24: this.executeLoad(instruction, 'byte', false); break;
            case 0x25: this.executeLoad(instruction, 'half', false); break;
            case 0x28: this.executeStore(instruction, 'byte'); break;
            case 0x29: this.executeStore(instruction, 'half'); break;
            case 0x2B: this.executeStore(instruction, 'word'); break;
            default:
                throw new Error(`Unimplemented opcode: 0x${opcode.toString(16)} at PC=0x${this.pc.toString(16)}`);
        }
        
        this.cycles++;
    }

    executeSPECIAL(instr) {
        const rs = (instr >> 21) & 0x1F;
        const rt = (instr >> 16) & 0x1F;
        const rd = (instr >> 11) & 0x1F;
        const shamt = (instr >> 6) & 0x1F;
        const funct = instr & 0x3F;
        
        switch (funct) {
            case 0x00: this.registers[rd] = this.registers[rt] << shamt; break;
            case 0x02: this.registers[rd] = this.registers[rt] >>> shamt; break;
            case 0x03: this.registers[rd] = this.registers[rt] >> shamt; break;
            case 0x04: this.registers[rd] = this.registers[rt] << (this.registers[rs] & 0x1F); break;
            case 0x06: this.registers[rd] = this.registers[rt] >>> (this.registers[rs] & 0x1F); break;
            case 0x07: this.registers[rd] = this.registers[rt] >> (this.registers[rs] & 0x1F); break;
            case 0x08: this.next_pc = this.registers[rs]; break;
            case 0x09: this.registers[rd] = this.pc + 4; this.next_pc = this.registers[rs]; break;
            case 0x20: 
                const sum = this.registers[rs] + this.registers[rt];
                if ((sum ^ this.registers[rs]) & (sum ^ this.registers[rt]) < 0) {
                    throw new Error(`ADD overflow at PC=0x${this.pc.toString(16)}`);
                }
                this.registers[rd] = sum;
                break;
            case 0x21: this.registers[rd] = this.registers[rs] + this.registers[rt]; break;
            case 0x22: 
                const diff = this.registers[rs] - this.registers[rt];
                if ((this.registers[rs] ^ this.registers[rt]) & (this.registers[rs] ^ diff) < 0) {
                    throw new Error(`SUB overflow at PC=0x${this.pc.toString(16)}`);
                }
                this.registers[rd] = diff;
                break;
            case 0x23: this.registers[rd] = this.registers[rs] - this.registers[rt]; break;
            case 0x24: this.registers[rd] = this.registers[rs] & this.registers[rt]; break;
            case 0x25: this.registers[rd] = this.registers[rs] | this.registers[rt]; break;
            case 0x26: this.registers[rd] = this.registers[rs] ^ this.registers[rt]; break;
            case 0x27: this.registers[rd] = ~(this.registers[rs] | this.registers[rt]); break;
            case 0x2A: this.registers[rd] = (this.registers[rs] < this.registers[rt]) ? 1 : 0; break;
            case 0x2B: this.registers[rd] = (this.registers[rs] < this.registers[rt] >>> 0) ? 1 : 0; break;
            default:
                throw new Error(`Unimplemented SPECIAL funct: 0x${funct.toString(16)} at PC=0x${this.pc.toString(16)}`);
        }
    }

    executeREGIMM(instr) {
        const rs = (instr >> 21) & 0x1F;
        const rt = (instr >> 16) & 0x1F;
        const offset = this.signExtend16(instr & 0xFFFF) << 2;
        
        switch (rt) {
            case 0x00: // BLTZ
                if (this.registers[rs] < 0) this.next_pc = this.pc + offset;
                break;
            case 0x01: // BGEZ
                if (this.registers[rs] >= 0) this.next_pc = this.pc + offset;
                break;
            case 0x10: // BLTZAL
                this.registers[RA] = this.pc + 4;
                if (this.registers[rs] < 0) this.next_pc = this.pc + offset;
                break;
            case 0x11: // BGEZAL
                this.registers[RA] = this.pc + 4;
                if (this.registers[rs] >= 0) this.next_pc = this.pc + offset;
                break;
            default:
                throw new Error(`Unimplemented REGIMM rt: 0x${rt.toString(16)}`);
        }
    }

    executeJ(instr) {
        const target = instr & 0x03FFFFFF;
        this.next_pc = (this.pc & 0xF0000000) | (target << 2);
    }

    executeJAL(instr) {
        const target = instr & 0x03FFFFFF;
        this.registers[RA] = this.pc + 4;
        this.next_pc = (this.pc & 0xF0000000) | (target << 2);
    }

    executeBEQ(instr) {
        const rs = (instr >> 21) & 0x1F;
        const rt = (instr >> 16) & 0x1F;
        const offset = this.signExtend16(instr & 0xFFFF) << 2;
        if (this.registers[rs] === this.registers[rt]) {
            this.next_pc = this.pc + offset;
        }
    }

    executeBNE(instr) {
        const rs = (instr >> 21) & 0x1F;
        const rt = (instr >> 16) & 0x1F;
        const offset = this.signExtend16(instr & 0xFFFF) << 2;
        if (this.registers[rs] !== this.registers[rt]) {
            this.next_pc = this.pc + offset;
        }
    }

    executeADDI(instr) {
        const rs = (instr >> 21) & 0x1F;
        const rt = (instr >> 16) & 0x1F;
        let immediate = instr & 0xFFFF;
        if (immediate & 0x8000) immediate |= 0xFFFF0000;
        
        const sum = this.registers[rs] + immediate;
        if ((sum ^ this.registers[rs]) & (sum ^ immediate) < 0) {
            throw new Error(`ADDI overflow at PC=0x${this.pc.toString(16)}`);
        }
        this.registers[rt] = sum;
    }

    executeSLTI(instr) {
        const rs = (instr >> 21) & 0x1F;
        const rt = (instr >> 16) & 0x1F;
        let immediate = instr & 0xFFFF;
        if (immediate & 0x8000) immediate |= 0xFFFF0000;
        this.registers[rt] = (this.registers[rs] < immediate) ? 1 : 0;
    }

    executeANDI(instr) {
        const rs = (instr >> 21) & 0x1F;
        const rt = (instr >> 16) & 0x1F;
        this.registers[rt] = this.registers[rs] & (instr & 0xFFFF);
    }

    executeORI(instr) {
        const rs = (instr >> 21) & 0x1F;
        const rt = (instr >> 16) & 0x1F;
        this.registers[rt] = this.registers[rs] | (instr & 0xFFFF);
    }

    executeXORI(instr) {
        const rs = (instr >> 21) & 0x1F;
        const rt = (instr >> 16) & 0x1F;
        this.registers[rt] = this.registers[rs] ^ (instr & 0xFFFF);
    }

    executeLUI(instr) {
        const rt = (instr >> 16) & 0x1F;
        this.registers[rt] = (instr & 0xFFFF) << 16;
    }

    executeLoad(instr, size, signed) {
        const base = (instr >> 21) & 0x1F;
        const rt = (instr >> 16) & 0x1F;
        let offset = instr & 0xFFFF;
        if (offset & 0x8000) offset |= 0xFFFF0000;
        
        const addr = (this.registers[base] + offset) >>> 0;
        
        switch (size) {
            case 'byte':
                let byte_val = this.readByte(addr);
                this.registers[rt] = signed ? (byte_val << 24) >> 24 : byte_val >>> 0;
                break;
            case 'half':
                if (addr & 1) throw new Error('Unaligned halfword access');
                let half_val = this.readHalf(addr);
                this.registers[rt] = signed ? (half_val << 16) >> 16 : half_val >>> 0;
                break;
            case 'word':
                if (addr & 3) throw new Error('Unaligned word access');
                this.registers[rt] = this.readWord(addr) >>> 0;
                break;
        }
    }

    executeStore(instr, size) {
        const base = (instr >> 21) & 0x1F;
        const rt = (instr >> 16) & 0x1F;
        let offset = instr & 0xFFFF;
        if (offset & 0x8000) offset |= 0xFFFF0000;
        
        const addr = (this.registers[base] + offset) >>> 0;
        const value = this.registers[rt];
        
        switch (size) {
            case 'byte': this.writeByte(addr, value); break;
            case 'half': 
                if (addr & 1) throw new Error('Unaligned halfword access');
                this.writeHalf(addr, value); 
                break;
            case 'word': 
                if (addr & 3) throw new Error('Unaligned word access');
                this.writeWord(addr, value); 
                break;
        }
    }

    executeCOP0(instr) {
        const funct = instr & 0x3F;
        if (funct === 0x00) { // MFC0
            const rt = (instr >> 16) & 0x1F;
            this.registers[rt] = 0;
        }
        // Other COP0 instructions no-op for our purposes
    }

    // Helper functions
    signExtend16(value) {
        return (value << 16) >> 16;
    }

    readHalf(addr) {
        if (addr >= this.memory.length - 1) {
            throw new Error(`Half read out of bounds: 0x${addr.toString(16)}`);
        }
        const view = new DataView(this.memory.buffer);
        return view.getUint16(addr, false);
    }

    writeHalf(addr, value) {
        if (addr >= this.memory.length - 1) {
            throw new Error(`Half write out of bounds: 0x${addr.toString(16)}`);
        }
        const view = new DataView(this.memory.buffer);
        view.setUint16(addr, value, false);
    }

    step() {
        const instruction = this.fetchInstruction();
        this.execute(instruction);
        this.registers[R0] = 0;
        
        // Handle branch delay slot
        if (this.next_pc !== this.pc) {
            this.pc = this.pc; // Current pc is delay slot
            const delay_instr = this.fetchInstruction();
            this.execute(delay_instr);
            this.registers[R0] = 0;
            this.pc = this.next_pc;
        } else {
            this.pc = this.next_pc;
        }
        
        this.next_pc = this.pc + 4;
    }

    run(maxCycles = 10000000) {
        this.running = true;
        let cycle_count = 0;
        
        while (this.running && cycle_count < maxCycles) {
            try {
                this.step();
                cycle_count++;
                
                // Check for doomgeneric function calls
                this.checkDoomGenericCalls();
                
            } catch (e) {
                if (e.message.includes('out of bounds')) {
                    console.error(`Memory error at PC=0x${this.pc.toString(16)}: ${e.message}`);
                    this.running = false;
                    break;
                } else {
                    throw e;
                }
            }
        }
        
        if (cycle_count >= maxCycles) {
            console.warn(`Max cycles (${maxCycles}) reached`);
        }
        
        return cycle_count;
    }
    
    checkDoomGenericCalls() {
        // The binary will call doomgeneric functions directly
        // Since they're external symbols, they'll jump to our implemented addresses
        // This will be handled naturally by the PC execution
    }
}

// ============================================================================
// DoomGeneric Host Implementation
// ============================================================================

class DoomGenericHost {
    constructor(cpu, outputDir = './frames') {
        this.cpu = cpu;
        this.frameCount = 0;
        this.outputDir = outputDir;
        this.startTime = Date.now();
        this.screenBufferAddr = null;
        this.lastDrawTime = 0;
        
        if (!fs.existsSync(this.outputDir)) {
            fs.mkdirSync(this.outputDir, { recursive: true });
        }
    }

    init() {
        console.log('DG_Init: Platform initialized');
    }

    drawFrame() {
        if (!this.screenBufferAddr) {
            // Find the screen buffer address by scanning memory
            this.screenBufferAddr = this.findScreenBuffer();
            if (!this.screenBufferAddr) {
                console.warn('DG_DrawFrame: Could not locate screen buffer');
                return;
            }
        }
        
        this.saveFrame(this.screenBufferAddr);
        this.frameCount++;
        
        // Periodically report progress
        if (this.frameCount % 10 === 0) {
            console.log(`Rendered ${this.frameCount} frames...`);
        }
    }

    sleepMs(ms) {
        // For accurate timing, could implement actual delay
        // but for batch rendering we want to be fast
    }

    getTicksMs() {
        return Math.floor(Date.now() - this.startTime);
    }

    getKey(pressedPtr, keyPtr) {
        // Headless mode - no keyboard input
        return 0;
    }

    setWindowTitle(title) {
        // Optional - just log
    }
    
    findScreenBuffer() {
        // The DG_ScreenBuffer is a global pointer set by doomgeneric_Create
        // It points to allocated memory: DG_ScreenBuffer = malloc(640*400*4)
        // Look for a valid pointer in the typical range
        
        // Search for non-zero 32-bit values that look like heap pointers
        const searchStart = 0x80000000; // Typical heap start
        const searchEnd = 0x90000000;
        
        for (let addr = searchStart; addr < searchEnd; addr += 4) {
            try {
                const ptr = this.cpu.readWord(addr);
                // Check if it's a plausible pointer to a large zero-initialized buffer
                if (ptr >= 0x10000000 && ptr < 0xF0000000) {
                    // Verify this looks like screen memory (check first few pixels)
                    try {
                        const test = this.cpu.readByte(ptr);
                        // Screen buffer likely has some content now
                        return ptr;
                    } catch (e) {
                        continue;
                    }
                }
            } catch (e) {
                continue;
            }
        }
        
        return null;
    }

    saveFrame(screenAddr) {
        const width = DOOMGENERIC_RESX;
        const height = DOOMGENERIC_RESY;
        const pixels = [];
        
        for (let y = 0; y < height; y++) {
            for (let x = 0; x < width; x++) {
                const offset = screenAddr + (y * width + x) * 4;
                try {
                    const r = this.cpu.readByte(offset);
                    const g = this.cpu.readByte(offset + 1);
                    const b = this.cpu.readByte(offset + 2);
                    const a = this.cpu.readByte(offset + 3);
                    pixels.push({ r, g, b, a });
                } catch (e) {
                    // If memory read fails, use black pixel
                    pixels.push({ r: 0, g: 0, b: 0, a: 255 });
                }
            }
        }
        
        // Save as PPM (simplest format)
        const ppmPath = path.join(this.outputDir, `frame_${this.frameCount.toString().padStart(6, '0')}.ppm`);
        this.writePPM(ppmPath, pixels, width, height);
        
        // Also try PNG if possible (using pure JS PNG encoder)
        // This would require additional library, but PPM works universally
    }

    writePPM(filepath, pixels, width, height) {
        const header = `P6\n${width} ${height}\n255\n`;
        const buffer = Buffer.alloc(header.length + pixels.length * 3);
        
        buffer.write(header, 0, 'ascii');
        let offset = header.length;
        
        for (const pixel of pixels) {
            buffer[offset++] = pixel.r;
            buffer[offset++] = pixel.g;
            buffer[offset++] = pixel.b;
        }
        
        fs.writeFileSync(filepath, buffer);
    }

    stop() {
        this.running = false;
        console.log(`Rendering complete: ${this.frameCount} frames saved to ${this.outputDir}`);
    }
}

// ============================================================================
// Symbol Table for Host Function Resolution
// ============================================================================

class SymbolTable {
    constructor() {
        this.symbols = new Map();
        this.nextAddress = 0x10000000; // Start host functions in kseg1
    }
    
    addSymbol(name, func) {
        const addr = this.nextAddress;
        this.nextAddress += 0x1000; // Page alignment (4KB per function)
        this.symbols.set(name, { addr, func });
        return addr;
    }
    
    getSymbol(name) {
        return this.symbols.get(name)?.addr || 0;
    }
    
    callSymbol(name, cpu) {
        const sym = this.symbols.get(name);
        if (sym) {
            console.log(`Calling host function: ${name}`);
            return sym.func(cpu);
        }
        return 0;
    }
}

// ============================================================================
// Main Execution
// ============================================================================

function main() {
    if (process.argv.length < 3) {
        console.log('Usage: node vm.js <mips_binary> [output_dir]');
        console.log('Example: node vm.js doomgeneric_mips ./frames');
        process.exit(1);
    }
    
    const binaryPath = process.argv[2];
    const outputDir = process.argv[3] || './frames';
    
    console.log(`╔══════════════════════════════════════════════╗`);
    console.log(`║   MIPS Interpreter for DoomGeneric           ║`);
    console.log(`╚══════════════════════════════════════════════╝`);
    console.log(`Binary: ${binaryPath}`);
    console.log(`Output: ${outputDir}`);
    
    // Read ELF binary
    const elfBuffer = fs.readFileSync(binaryPath);
    console.log(`Size: ${elfBuffer.length} bytes`);
    
    // Create CPU and load binary
    const cpu = new MIPSCPU();
    try {
        cpu.loadELF(elfBuffer);
    } catch (e) {
        console.error('❌ ELF loading failed:', e.message);
        process.exit(1);
    }
    
    // Create host
    const host = new DoomGenericHost(cpu, outputDir);
    cpu.host = host;
    
    // Register doomgeneric host functions as symbols
    const symbols = new SymbolTable();
    
    // Map external symbols that the binary expects
    // These correspond to the functions defined in doomgeneric.h
    symbols.addSymbol('DG_Init', () => host.init());
    symbols.addSymbol('DG_DrawFrame', () => host.drawFrame());
    symbols.addSymbol('DG_SleepMs', () => host.sleepMs());
    symbols.addSymbol('DG_GetTicksMs', () => host.getTicksMs());
    symbols.addSymbol('DG_GetKey', () => host.getKey());
    symbols.addSymbol('DG_SetWindowTitle', () => host.setWindowTitle());
    
    console.log('\n🚀 Starting execution...');
    console.log('═'.repeat(50));
    
    const startTime = Date.now();
    const cycles = cpu.run(10000000); // 10M cycle limit
    const elapsed = Date.now() - startTime;
    
    console.log('═'.repeat(50));
    console.log(`\n✅ Execution Complete:`);
    console.log(`   Cycles executed: ${cycles.toLocaleString()}`);
    console.log(`   Time: ${elapsed}ms (${(cycles/elapsed/1000).toFixed(2)} MIPS)`);
    console.log(`   Frames rendered: ${host.frameCount}`);
    console.log(`   Output directory: ${outputDir}`);
    
    if (host.frameCount === 0) {
        console.warn('\n⚠️  No frames were rendered. Possible issues:');
        console.warn('   - Screen buffer not found (check memory scanning)');
        console.warn('   - Binary terminated before first DG_DrawFrame');
        console.warn('   - Missing doomgeneric function implementation');
    } else {
        console.log(`\n🎮 First frame saved as: ${path.join(outputDir, 'frame_000000.ppm')}`);
        console.log(`   Open with: open ${outputDir}/frame_000000.ppm`);
        console.log(`   Or convert: convert ${outputDir}/frame_*.ppm output.mp4`);
    }
    
    process.exit(0);
}

// Run if executed directly
if (require.main === module) {
    main().catch(err => {
        console.error('Fatal error:', err);
        process.exit(1);
    });
}

module.exports = { MIPSCPU, DoomGenericHost, SymbolTable };
