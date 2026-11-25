//! Tests de integración del ensamblador con programas completos.

use rv_asm::{AsmError, Program, assemble};
use rv_isa::{INSTRUCTIONS, Instruction, Operands, PSEUDO_INSTRUCTIONS, RegisterStyle};

fn ok(source: &str) -> Program {
    match assemble(source) {
        Ok(program) => program,
        Err(errors) => panic!("errores inesperados: {errors:#?}"),
    }
}

fn errors(source: &str) -> Vec<AsmError> {
    match assemble(source) {
        Ok(program) => panic!("debía fallar y ensambló: {:#?}", program.listing),
        Err(errors) => errors,
    }
}

fn words(program: &Program) -> Vec<u32> {
    program.listing.iter().map(|entry| entry.word).collect()
}

fn disassembly(program: &Program) -> Vec<String> {
    program
        .listing
        .iter()
        .map(|entry| {
            entry
                .instruction
                .disassemble(entry.address, RegisterStyle::Abi)
        })
        .collect()
}

#[test]
fn assembles_a_loop_with_known_encodings() {
    let program = ok("
        # suma 1..10
        .text
        main:
            li   t0, 10
            li   a0, 0
        loop:
            add  a0, a0, t0
            addi t0, t0, -1
            bnez t0, loop
            ecall
    ");
    assert_eq!(
        words(&program),
        [
            0x00a0_0293,
            0x0000_0513,
            0x0055_0533,
            0xfff2_8293,
            0xfe02_9ce3,
            0x0000_0073
        ]
    );
    assert_eq!(program.labels.get("loop"), Some(&8));
    assert_eq!(program.line_at(16), Some(10));
    assert_eq!(program.listing[4].pseudo, Some("bnez"));
    assert_eq!(program.listing[2].pseudo, None);
}

#[test]
fn forward_references_and_absolute_targets() {
    let program = ok("j end\nnop\nend: jal ra, end");
    assert_eq!(
        disassembly(&program),
        ["jal zero, 0x8", "addi zero, zero, 0", "jal ra, 0x8"]
    );
}

#[test]
fn data_section_with_alignment_strings_and_la() {
    let program = ok(r#"
        .data
        msg:   .string "hola"
        value:
               .word 0x12345678, -1
        half:  .half 7
        bytes: .byte 1, 'A', 255
        .text
               la a0, value
               lw a1, value
               sw a1, half, t0
    "#);
    let data = &program.data;
    assert_eq!(data.base, 0x1000_0000);
    assert_eq!(&data.bytes[..5], b"hola\0");
    // value se alinea a 4 aunque la etiqueta esté en la línea anterior.
    assert_eq!(program.labels.get("value"), Some(&0x1000_0008));
    assert_eq!(
        &data.bytes[8..16],
        &[0x78, 0x56, 0x34, 0x12, 0xff, 0xff, 0xff, 0xff]
    );
    assert_eq!(program.labels.get("half"), Some(&0x1000_0010));
    assert_eq!(&data.bytes[16..21], &[7, 0, 1, b'A', 255]);
    assert_eq!(
        disassembly(&program),
        [
            "auipc a0, 0x10000",
            "addi a0, a0, 8",
            "auipc a1, 0x10000",
            "lw a1, 0(a1)",
            "auipc t0, 0x10000",
            "sw a1, 0(t0)",
        ]
    );
}

/// Simula `lui` + `addi` para comprobar que `li` carga el valor pedido.
fn li_result(source: &str) -> (u32, usize) {
    let program = ok(source);
    let mut value = 0u32;
    for entry in &program.listing {
        let Instruction { imm, .. } = entry.instruction;
        value = match entry.instruction.mnemonic {
            rv_isa::Mnemonic::Lui => imm as u32,
            rv_isa::Mnemonic::Addi => value.wrapping_add(imm as u32),
            other => panic!("li no debería usar {other:?}"),
        };
    }
    (value, program.listing.len())
}

#[test]
fn li_picks_the_shortest_sequence() {
    assert_eq!(li_result("li a0, 5"), (5, 1));
    assert_eq!(li_result("li a0, -2048"), (0xffff_f800, 1));
    assert_eq!(li_result("li a0, 2048"), (2048, 2));
    assert_eq!(li_result("li a0, 0x12345000"), (0x1234_5000, 1));
    assert_eq!(li_result("li a0, 0x12345fff"), (0x1234_5fff, 2));
    assert_eq!(li_result("li a0, 0xffffffff"), (0xffff_ffff, 1));
    assert_eq!(li_result("li a0, 0x80000000"), (0x8000_0000, 1));
    assert_eq!(li_result("li a0, -2147483648"), (0x8000_0000, 1));
    assert_eq!(li_result(".equ N, 100000\nli a0, N + 1"), (100_001, 2));
}

#[test]
fn every_pseudo_instruction_example_assembles() {
    for pseudo in &PSEUDO_INSTRUCTIONS {
        let source = format!("{}\nlabel: nop\n", pseudo.example);
        let program = ok(&source);
        let first = &program.listing[0];
        assert_eq!(first.pseudo, Some(pseudo.name), "{}", pseudo.example);
    }
}

/// Inmediatos positivos para que el destino de los saltos, absoluto, no se salga de la memoria.
fn sample_source(spec: &rv_isa::InstructionSpec) -> String {
    let name = spec.name;
    match spec.operands {
        Operands::RdRs1Rs2 => format!("{name} s1, t2, a7"),
        Operands::RdRs1Imm => format!("{name} s1, t2, -77"),
        Operands::RdRs1Shamt => format!("{name} s1, t2, 19"),
        Operands::RdOffsetRs1 => format!("{name} s1, -12(t2)"),
        Operands::Rs2OffsetRs1 => format!("{name} s1, 2047(t2)"),
        Operands::Rs1Rs2Target => format!("{name} s1, t2, 0x40"),
        Operands::RdUpper => format!("{name} s1, 0xfffff"),
        Operands::RdTarget => format!("{name} s1, 0x800"),
        Operands::Fence => format!("{name} rw, io"),
        Operands::None => name.to_owned(),
    }
}

#[test]
fn every_real_instruction_round_trips_through_text() {
    for spec in &INSTRUCTIONS {
        let program = ok(&sample_source(spec));
        let entry = &program.listing[0];
        assert_eq!(entry.instruction.mnemonic, spec.mnemonic);
        let text = entry
            .instruction
            .disassemble(entry.address, RegisterStyle::Numeric);
        let again = ok(&text);
        assert_eq!(words(&again), words(&program), "{} -> {text}", spec.name);
    }
}

#[test]
fn hi_lo_and_constants() {
    let program = ok("
        .equ BASE, 0x12345FFF
        lui  a0, %hi(BASE)
        addi a0, a0, %lo(BASE)
    ");
    let imm: Vec<i32> = program
        .listing
        .iter()
        .map(|entry| entry.instruction.imm)
        .collect();
    assert_eq!(imm, [0x1234_6000, -1]);
}

#[test]
fn reports_errors_with_line_and_column() {
    let errs = errors("addi a0, a0, 5000\nfoo a0\n  add a0, a1\nbeq a0, a1, missing");
    let found: Vec<(usize, usize)> = errs.iter().map(|e| (e.line, e.column)).collect();
    assert_eq!(found, [(1, 14), (2, 1), (3, 3), (4, 13)]);
    assert!(errs[1].message.contains("foo"));
    assert!(errs[2].message.contains("add rd, rs1, rs2"));

    let errs = errors("addi a0, a0, 5000");
    assert_eq!((errs[0].line, errs[0].column, errs[0].length), (1, 14, 4));
    assert!(errs[0].message.contains("-2048"));

    let errs = errors("beq a0, a1, missing");
    assert!(errs[0].message.contains("missing"));
    assert_eq!(errs[0].column, 13);
}

#[test]
fn rejects_semantic_mistakes() {
    let cases = [
        ("x: nop\nx: nop", "ya está definido"),
        ("a0: nop", "es un registro"),
        (".data\nadd a0, a0, a0", ".text"),
        ("li a0, label\nlabel: nop", "constante"),
        ("label: nop\nli a0, label", "es una etiqueta"),
        ("li a0, 0x1ffffffff", "32 bits"),
        ("lui a0, 0x100000", "20 bits"),
        ("slli a0, a0, 32", "fuera de rango"),
        (".word", "al menos un valor"),
        (".byte 256", "no cabe"),
        (".align 13", "0 a 12"),
        (".foo 1", "directiva desconocida"),
        ("fence.i", "no está admitida"),
        ("lw a0, 4(a1), 3", "se usa así"),
        (".space 0x2000000", "tamaño"),
        ("beq a0, a1, far\n.space 8192\nfar: nop", "fuera de rango"),
        (".string 5", "cadena"),
        ("ret a0", "se usa así"),
        ("fence rx, w", "iorw"),
    ];
    for (source, fragment) in cases {
        let errs = errors(source);
        assert!(
            errs.iter().any(|error| error.message.contains(fragment)),
            "{source:?}: {errs:#?}"
        );
    }
}

#[test]
fn never_panics_on_garbage() {
    let fragments = [
        "add",
        "a0",
        ",",
        "(",
        ")",
        "%hi",
        "%lo(",
        "label:",
        ".word",
        "\"x",
        "'",
        "-",
        "+",
        "0x",
        "99999999999",
        ".data",
        ".text",
        "lw",
        "sp",
        "#",
        "jal",
        ":",
        "li",
        "la",
        "\\",
        "ñ",
    ];
    // Combinaciones deterministas de fragmentos: lo que importa es que nunca haya pánico.
    let mut seed = 0x2545_f491u32;
    for _ in 0..4000 {
        let mut line = String::new();
        for _ in 0..(seed % 7) {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            line.push_str(fragments[(seed as usize) % fragments.len()]);
            line.push(if seed & 1 == 0 { ' ' } else { ',' });
        }
        let _ = assemble(&line);
        seed = seed.wrapping_add(0x9e37_79b9);
    }
}
