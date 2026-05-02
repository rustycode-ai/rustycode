import { describe, it, expect } from "vitest";
import { splitByFilePaths, fuzzyMatch } from "../../utils/text";

describe("splitByFilePaths", () => {
  it("returns single non-highlighted segment for text without file paths", () => {
    const result = splitByFilePaths("no paths here");
    expect(result).toEqual([{ text: "no paths here", highlight: false }]);
  });

  it("returns single non-highlighted segment for empty string", () => {
    const result = splitByFilePaths("");
    expect(result).toEqual([{ text: "", highlight: false }]);
  });

  it("highlights a single file path", () => {
    const result = splitByFilePaths("error in src/main.rs");
    expect(result).toEqual([
      { text: "error in ", highlight: false },
      { text: "src/main.rs", highlight: true },
    ]);
  });

  it("highlights file path with line number", () => {
    const result = splitByFilePaths("src/main.rs:42");
    expect(result).toEqual([{ text: "src/main.rs:42", highlight: true }]);
  });

  it("highlights file path with line:col", () => {
    const result = splitByFilePaths("src/app.ts:10:5");
    expect(result).toEqual([{ text: "src/app.ts:10:5", highlight: true }]);
  });

  it("highlights multiple file paths", () => {
    const result = splitByFilePaths("changed src/main.rs and lib/utils.ts");
    expect(result).toEqual([
      { text: "changed ", highlight: false },
      { text: "src/main.rs", highlight: true },
      { text: " and ", highlight: false },
      { text: "lib/utils.ts", highlight: true },
    ]);
  });

  it("highlights various file extensions", () => {
    const exts = ["file.rs", "file.ts", "file.tsx", "file.js", "file.jsx", "file.py", "file.toml", "file.yaml", "file.json", "file.go", "file.java", "file.cpp"];
    for (const fp of exts) {
      const result = splitByFilePaths(fp);
      expect(result).toEqual([{ text: fp, highlight: true }]);
    }
  });

  it("highlights .json as whole extension (not greedily .js first)", () => {
    const result = splitByFilePaths("config.json");
    expect(result).toEqual([{ text: "config.json", highlight: true }]);
  });

  it("handles file path at start of text", () => {
    const result = splitByFilePaths("src/main.rs has an error");
    expect(result).toEqual([
      { text: "src/main.rs", highlight: true },
      { text: " has an error", highlight: false },
    ]);
  });

  it("handles file path at end of text", () => {
    const result = splitByFilePaths("see src/main.rs");
    expect(result).toEqual([
      { text: "see ", highlight: false },
      { text: "src/main.rs", highlight: true },
    ]);
  });

  it("does not match file path without recognized extension", () => {
    const result = splitByFilePaths("see readme.txt");
    expect(result).toEqual([{ text: "see readme.txt", highlight: false }]);
  });
});

describe("fuzzyMatch", () => {
  it("matches exact substring", () => {
    expect(fuzzyMatch("test", "this is a test")).toBe(true);
  });

  it("matches case-insensitive substring", () => {
    expect(fuzzyMatch("TEST", "this is a test")).toBe(true);
    expect(fuzzyMatch("test", "THIS IS A TEST")).toBe(true);
  });

  it("matches fuzzy characters in order", () => {
    expect(fuzzyMatch("fb", "foo bar")).toBe(true);
    expect(fuzzyMatch("fbr", "foo bar")).toBe(true);
  });

  it("returns false for non-matching", () => {
    expect(fuzzyMatch("xyz", "foo bar")).toBe(false);
  });

  it("handles empty query (matches anything)", () => {
    expect(fuzzyMatch("", "anything")).toBe(true);
    expect(fuzzyMatch("", "")).toBe(true);
  });

  it("returns false when query is longer than text", () => {
    expect(fuzzyMatch("longer query", "short")).toBe(false);
  });

  it("matches single character", () => {
    expect(fuzzyMatch("a", "abc")).toBe(true);
    expect(fuzzyMatch("d", "abc")).toBe(false);
  });

  it("fuzzy matches across words", () => {
    expect(fuzzyMatch("tdd", "TDD Guide")).toBe(true);
    expect(fuzzyMatch("fd", "Frontend Design")).toBe(true);
    expect(fuzzyMatch("fgd", "Frontend Design")).toBe(false);
  });
});
