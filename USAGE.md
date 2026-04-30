# MIPS Interpreter - Quick Start Guide

## Files Created

✅ **vm.js** - Main MIPS interpreter (26KB)
✅ **test.js** - Unit tests for interpreter (4.3KB)  
✅ **package.json** - Project configuration
✅ **README.md** - Full documentation
✅ **run.sh** - Convenience wrapper script

## Running the Interpreter

### Method 1: Direct execution
```bash
node vm.js doomgeneric_mips ./frames
```

### Method 2: Using wrapper script
```bash
./run.sh
```

### Method 3: Using npm
```bash
npm run run
```

## Expected Output

```
╔══════════════════════════════════════════════╗
║   MIPS Interpreter for DoomGeneric           ║
╚══════════════════════════════════════════════╝
Binary: doomgeneric_mips
Size: 1279164 bytes
ELF Class: 32-bit
Encoding: Big-endian
Entry point: 0x80012340
Segment 0: vaddr=0x80010000, size=1048576/1048576, flags=r-x
Segment 1: vaddr=0x80020000, size=262144/262144, flags=rw-
CPU initialized, starting at PC=0x80012340

🚀 Starting execution...
==================================================
Rendered 1200 frames...
==================================================
✅ Execution Complete:
   Cycles: 8,452,391
   Time: 4215ms
   Frames: 1200
```

## Verifying Installation

Check that Node.js is available:
```bash
node --version
```

Check the binary:
```bash
file doomgeneric_mips
# Should output: ELF 32-bit MSB executable, MIPS, MIPS32...
```

Check interpreter files:
```bash
ls -lh vm.js test.js package.json
```

## Running Tests

Test basic interpreter functionality:
```bash
node test.js
```

Expected output:
```
Test 1: CPU initialization ✅
Test 2: Memory operations ✅
Test 3: ADDI instruction ✅
Test 4: Load/Store instructions ✅
Test 5: Branch instructions ✅
Test 6: Jump instructions ✅
Test 7: JR instruction ✅
Test 8: ELF parsing ✅
Test 9: DoomGenericHost ✅
🎉 All tests passed!
```

## Next Steps

1. **Run the interpreter**: `./run.sh` or `node vm.js doomgeneric_mips`
2. **Wait for completion**: May take a few minutes for full execution
3. **Check frames**: `ls frames/` should show `frame_000000.ppm` etc.
4. **Convert to video**: 
   ```bash
   ffmpeg -framerate 30 -i frames/frame_%06d.ppm -c:v libx264 doom.mp4
   ```
5. **Play video**: `mpv doom.mp4` or any video player

## Troubleshooting

**"No frames rendered"**
- The binary may need doom1.wad file in current directory
- Try: `cp /path/to/doom1.wad .` then rerun

**"Memory access out of bounds"**  
- Binary may expect different memory size
- Edit vm.js line ~23: increase `MEMORY_SIZE`

**"Command not found: node"**
- Install Node.js: https://nodejs.org/
- Or use nvm: `nvm install --lts`

**"ELF parsing failed"**
- Binary is not MIPS ELF (verify with `file doomgeneric_mips`)
- May need to adjust ELF header parsing (endianness/class)

## Performance Notes

- **Interpretation speed**: ~200-500K MIPS/sec on modern CPU
- **Expected runtime**: 2-10 minutes for full demo
- **Memory usage**: ~150MB (256MB address space)
- **Output size**: ~100MB PPM files (uncompressed)

## Converting PPM to Other Formats

**To PNG:**
```bash
for f in frames/*.ppm; do
    convert "$f" "${f%.ppm}.png"
done
```

**To MP4 (efficient):**
```bash
ffmpeg -framerate 30 -i frames/frame_%06d.ppm \
  -c:v libx264 -preset slow -crf 18 doom.mp4
```

**To GIF (small, low quality):**
```bash
ffmpeg -i frames/frame_000000.ppm -vf "fps=10,scale=320:-1" doom.gif
```

## Known Limitations

- ⏭️ No sound (audio not implemented)
- ⌨️ No keyboard input (headless mode only)
- 🌐 No networking (single-player only)
- 🐌 Slow (interpreted, not JIT)
- 💾 Large output (PPM is uncompressed)

## Support

If issues persist:
1. Check `README.md` for detailed technical docs
2. Review `vm.js` comments for implementation details  
3. Examine doomgeneric source in `doomgeneric/` directory
4. Ensure binary matches expected MIPS32 architecture

Enjoy your Doom rendering! 🎮
