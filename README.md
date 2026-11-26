# rvscope

A RISC-V simulator for learning and writing assembly, built from scratch in Rust and compiled to WebAssembly so it runs entirely in the browser.

## What it is

rvscope is a tool for students and anyone learning computer architecture with RISC-V: write a program, assemble it and inspect exactly what each line turns into. It is designed around three ideas:

- **Everything runs on the client.** The core is a Rust library compiled to WebAssembly; the web app is static files with no backend.
- **One instruction table rules everything.** The encoder, decoder, assembler and the in-app instruction reference are all generated from the same table, so the reference can never describe an instruction the simulator does not support.
- **Deterministic and honest errors.** The same source always produces the same bytes, and a badly written program is a list of errors with line and column, never a crash.

The ISA covered is RV32I plus the M extension (multiply and divide).

## What makes it interesting

- **A real two-pass assembler**, not a regex translator: tokenizer with source spans, operand and expression parser with C precedence (`label+4`, `(1 << 31) - 1`, `%hi(sym)`, `%lo(sym)(a0)`, `.equ` constants), numeric local labels (`1:` / `1b` / `1f`), several statements per line with `;`, sections, data directives with natural alignment, and pseudo-instruction expansion with the shortest correct sequence (`li` picks one or two instructions depending on the constant).
- **Conformance with the official [riscv-tests](https://github.com/riscv-software-src/riscv-tests)**: the 40 applicable rv32ui tests and the 8 rv32um tests are assembled by rvscope's own assembler and run on every processor model in CI. They are preprocessed once from the upstream sources with a small replacement for the test environment, so neither CI nor contributors need a RISC-V toolchain.
- **Bit-exact encoding and decoding** for every instruction format (R, I, S, B, U, J), checked with encode → decode → disassemble → re-assemble round trips over every instruction in the table.
- **Error reporting built for an editor**: every error carries line, column and length, all errors in a file are reported at once, and a fuzz-style test throws thousands of garbage lines at the assembler to prove it never panics.
- **A strict boundary between core and UI**: only one crate knows about `wasm-bindgen`; everything else is plain Rust, tested natively and usable from the command line.

## How it works

1. **`rv-isa`** holds the instruction table: mnemonic, format, opcode/funct fields, operand shape, a one-line description and a pseudocode summary for each of the 48 RV32IM instructions, plus the pseudo-instruction list and the register file with its ABI calling convention.
2. **`rv-asm`** reads the source line by line. The first pass places every statement in `.text` or `.data` and binds labels (a label on its own line attaches to the next item, after alignment). The second pass evaluates operands, expands pseudo-instructions, encodes them through `rv-isa` and builds a listing that maps every address back to its source line.
3. **`rv-cli`** exposes the assembler and the single-cycle simulator from a terminal; **`rv-wasm`** exposes the core to the web app.

## Architecture

```text
rvscope/
├── Cargo.toml            Cargo workspace (edition 2024)
├── crates/
│   ├── rv-isa/           instruction table, registers, formats, encoder, decoder, disassembler
│   ├── rv-asm/           lexer, parser, expression evaluator, pseudo-instruction expansion, two-pass assembler
│   ├── rv-core/          processor models, memory and system calls
│   ├── rv-wasm/          wasm-bindgen facade used by the web app
│   └── rv-cli/           `rvscope` command-line tool
├── web/                  Vite + strict TypeScript app that loads the wasm core
├── examples/             sample assembly programs
└── docs/decisions/       architecture decision records
```

## Requirements

- Rust stable (1.85 or newer) with the `wasm32-unknown-unknown` target
- [`wasm-pack`](https://github.com/rustwasm/wasm-pack) 0.13 or newer
- Node.js 22.12 or newer

```bash
rustup target add wasm32-unknown-unknown
```

## Usage

### Command line

Assemble a file and print the listing (address, machine word, decoded instruction, source line):

```bash
cargo run -p rv-cli -- asm examples/factorial.s
```

```text
00000000  10000517  auipc a0, 0x10000               7: lw    a0, n
00000004  00052503  lw a0, 0(a0)                    7: lw    a0, n
00000008  00000097  auipc ra, 0x0                   8: call  factorial
0000000c  010080e7  jalr ra, 16(ra)                 8: call  factorial
00000010  00a00893  addi a7, zero, 10               9: li    a7, 10            # salir
...
00000028  00a2de63  bge t0, a0, 0x44               18: ble   a0, t0, base
```

Errors are reported in the usual `file:line:column` form, all at once:

```text
bad.s:1:14: addi: el inmediato 9000 está fuera de rango (-2048 a 2047)
bad.s:2:1: instrucción desconocida «bogus»
```

Run a program on the single-cycle processor. What the program prints goes to stdout, what it reads (system calls 5, 8 and 12) comes from stdin, and the stop reason, the source line it happened on and the register file go to stderr, so redirecting stdout keeps only the program's own output:

```bash
cargo run -p rv-cli -- run examples/factorial.s
```

```text
parada en la línea 10: 0x00000014: el programa terminó con código 0
ciclos: 85  instrucciones: 85  pc: 0x00000018
zero 0x00000000     ra 0x00000010     sp 0x7ffffff0     gp 0x00000000
  tp 0x00000000     t0 0x00000001     t1 0x00000006     t2 0x00000000
  s0 0x00000000     s1 0x00000000     a0 0x000002d0     a1 0x00000000
...
```

`--trace` adds one line per retired instruction (cycle, address, word, source line, disassembly and what it changed), `--numeric` names registers `x0`–`x31`, and `--max-cycles <n>` caps the run (10 000 000 by default) so an infinite loop ends with a message instead of hanging. The process exit code is the one the program passed to its exit system call; a fault or an unfinished run exits with 1.

List every supported instruction and pseudo-instruction, grouped:

```bash
cargo run -p rv-cli -- isa
```

The user-facing text (messages, descriptions) is in Spanish, the language of the course the tool was born for.

### Web app

```bash
cd web
npm install
npm run wasm     # builds crates/rv-wasm into web/src/wasm/pkg
npm run dev
```

### Supported assembly

- Registers by number (`x0`–`x31`) or ABI name (`zero`, `ra`, `sp`, `a0`, `fp`…).
- Numbers in decimal, `0x` hex, `0b` binary, `0o` octal and character literals (`'A'`, `'\n'`).
- Sections: `.text`, `.data`, `.rodata`, `.bss`, `.section`.
- Data: `.byte`, `.half`, `.word`, `.string` / `.asciz`, `.ascii`, `.space` / `.zero`, `.align`, `.balign`.
- Symbols: labels, numeric local labels (`1:` referenced as `1b` backwards or `1f` forwards), `.equ` / `.set` constants, `.globl`.
- Expressions: `+ - * /`, `<< >>`, `& | ^`, unary `-` and `~`, parentheses, evaluated in 64-bit two's complement like GNU as (so masks such as `0xffffffff80000000 & 0xffffffff` work); the instruction or directive then checks that the result fits.
- Relocation modifiers: `%hi(expr)` and `%lo(expr)`.
- Several statements on one line separated by `;`, and `.option` (accepted for compatibility with compiler output; it changes nothing).
- Register-register ALU instructions with an immediate operand assemble as their immediate form, as in GNU as (`add a0, a1, 5` is `addi`).
- Pseudo-instructions: `nop`, `li`, `la`, `lla`, `mv`, `not`, `neg`, `seqz`, `snez`, `sltz`, `sgtz`, `beqz`, `bnez`, `blez`, `bgez`, `bltz`, `bgtz`, `bgt`, `ble`, `bgtu`, `bleu`, `j`, `jal label`, `jr rs[, imm]`, `jalr rs`, `ret`, `call`, `tail`, and the global load/store forms (`lw a0, label`, `sw a0, label, t0`).

Memory layout: `.text` starts at `0x00000000` and `.data` at `0x10000000`. Alignment gaps inside `.text` are filled with `nop`, so code after `.align` runs as expected.

## Using the crates

```rust
use rv_asm::assemble;
use rv_isa::RegisterStyle;

let program = assemble("li a0, 100000\necall").map_err(|errors| errors[0].to_string())?;
for entry in &program.listing {
    println!("{:08x} {}", entry.address, entry.instruction.disassemble(entry.address, RegisterStyle::Abi));
}
```

`Program` contains the `.text` and `.data` segments as bytes, the label table and the listing; `Program::line_at(address)` and `Program::addresses_of_line(line)` map between machine code and source.

## Development

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cd web
npm run wasm && npm run lint && npm run typecheck && npm run build
```

CI runs the same checks on every push to `main`.

The riscv-tests live preprocessed in `crates/rv-core/tests/riscv-tests` (with their BSD license and the upstream commit). To refresh them from a newer upstream checkout:

```bash
git clone https://github.com/riscv-software-src/riscv-tests /tmp/riscv-tests
scripts/vendor-riscv-tests.sh /tmp/riscv-tests
cargo test -p rv-core --test riscv_tests
```

`fence_i` (needs Zifencei) and `ma_data` (misaligned accesses, which rvscope reports as an error) are left out.

## Troubleshooting

- **`wasm-pack: command not found`** — install it from its release page or with `cargo install wasm-pack`, and make sure `~/.cargo/bin` is on your `PATH`.
- **The web app shows "no se pudo cargar el núcleo"** — the wasm package has not been built; run `npm run wasm` inside `web/` before `npm run dev`.
- **`error: can't find crate for core` when building the wasm** — the WebAssembly target is missing: `rustup target add wasm32-unknown-unknown`.
- **`li a0, label` is rejected** — `li` loads constants; use `la a0, label` to load an address.

## License

[MIT](LICENSE) © juanmmm21

The preprocessed riscv-tests in `crates/rv-core/tests/riscv-tests` are © The Regents of the University of California and distributed under their own [BSD license](crates/rv-core/tests/riscv-tests/LICENSE).
