//! Contenido de la chuleta de instrucciones: todo sale de las tablas que usan el ensamblador y el
//! simulador (`rv-isa`, `rv-asm` y las ecalls de `rv-core`), así que la chuleta no puede anunciar
//! nada que rvscope no admita ni olvidarse de algo que sí admite.

use rv_asm::{DIRECTIVES, DirectiveSpec, assemble};
use rv_core::{SYSCALLS, SyscallInfo};
use rv_isa::{
    EncodedField, Extension, Format, Group, INSTRUCTIONS, InstructionSpec, PSEUDO_INSTRUCTIONS,
    PseudoSpec, REGISTERS, RegisterInfo,
};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct CheatSheet {
    /// En el orden de [`Group::ALL`] y, dentro de cada grupo, en el de la tabla.
    pub groups: Vec<CheatGroup>,
    pub pseudo_instructions: &'static [PseudoSpec],
    pub directives: &'static [DirectiveSpec],
    pub syscalls: &'static [SyscallInfo],
    pub registers: &'static [RegisterInfo],
}

#[derive(Clone, Debug, Serialize)]
pub struct CheatGroup {
    pub id: Group,
    pub title: &'static str,
    pub instructions: Vec<CheatInstruction>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CheatInstruction {
    pub name: &'static str,
    pub syntax: String,
    pub summary: &'static str,
    pub operation: &'static str,
    pub format: Format,
    pub extension: Extension,
    /// Campos de bits del más alto al más bajo, con los valores fijos que la identifican.
    pub fields: Vec<EncodedField>,
    pub example: &'static str,
    /// Palabra en que se ensambla `example`, para enseñar sus bits campo a campo.
    pub example_word: Option<u32>,
}

pub fn cheat_sheet() -> CheatSheet {
    let groups = Group::ALL
        .iter()
        .map(|&group| CheatGroup {
            id: group,
            title: group.title(),
            instructions: INSTRUCTIONS
                .iter()
                .filter(|spec| spec.group == group)
                .map(cheat_instruction)
                .collect(),
        })
        .collect();
    CheatSheet {
        groups,
        pseudo_instructions: &PSEUDO_INSTRUCTIONS,
        directives: &DIRECTIVES,
        syscalls: SYSCALLS,
        registers: &REGISTERS,
    }
}

fn cheat_instruction(spec: &'static InstructionSpec) -> CheatInstruction {
    CheatInstruction {
        name: spec.name,
        syntax: spec.syntax(),
        summary: spec.summary,
        operation: spec.operation,
        format: spec.format,
        extension: spec.extension,
        fields: spec.fields(),
        example: spec.example(),
        example_word: example_word(spec),
    }
}

// El ejemplo se ensambla de verdad en vez de codificarse a mano: así la ficha enseña lo mismo
// que saldría en el editor. Los saltos apuntan a `label`, que va justo detrás.
fn example_word(spec: &InstructionSpec) -> Option<u32> {
    let source = format!("{}\nlabel: nop\n", spec.example());
    let program = assemble(&source).ok()?;
    program
        .listing
        .first()
        .filter(|entry| entry.instruction.mnemonic == spec.mnemonic)
        .map(|entry| entry.word)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn every_instruction_appears_exactly_once() {
        let sheet = cheat_sheet();
        let mut seen = HashSet::new();
        for group in &sheet.groups {
            assert!(!group.instructions.is_empty(), "{:?} vacío", group.id);
            for instruction in &group.instructions {
                assert!(
                    seen.insert(instruction.name),
                    "repetida: {}",
                    instruction.name
                );
            }
        }
        assert_eq!(seen.len(), INSTRUCTIONS.len());
    }

    #[test]
    fn groups_follow_the_display_order() {
        let ids: Vec<Group> = cheat_sheet().groups.iter().map(|group| group.id).collect();
        assert_eq!(ids, Group::ALL.to_vec());
    }

    #[test]
    fn extension_m_is_only_in_its_groups() {
        for group in cheat_sheet().groups {
            let expect_m = matches!(group.id, Group::Multiply | Group::Divide);
            for instruction in group.instructions {
                assert_eq!(
                    instruction.extension == Extension::M,
                    expect_m,
                    "{}",
                    instruction.name
                );
            }
        }
    }

    #[test]
    fn tables_are_complete() {
        let sheet = cheat_sheet();
        assert_eq!(sheet.pseudo_instructions.len(), PSEUDO_INSTRUCTIONS.len());
        assert_eq!(sheet.directives.len(), DIRECTIVES.len());
        assert_eq!(sheet.syscalls.len(), SYSCALLS.len());
        assert_eq!(sheet.registers.len(), 32);
    }

    #[test]
    fn every_instruction_has_its_card() {
        for group in cheat_sheet().groups {
            for instruction in group.instructions {
                assert!(
                    instruction.example.starts_with(instruction.name),
                    "{}",
                    instruction.name
                );
                assert!(instruction.example_word.is_some(), "{}", instruction.name);
                assert!(!instruction.fields.is_empty(), "{}", instruction.name);
            }
        }
    }

    #[test]
    fn addi_card_shows_the_example_bits() {
        let sheet = cheat_sheet();
        let addi = sheet
            .groups
            .iter()
            .flat_map(|group| &group.instructions)
            .find(|instruction| instruction.name == "addi");
        let Some(addi) = addi else {
            panic!("falta addi");
        };
        assert_eq!(addi.example, "addi sp, sp, -16");
        // imm = -16 (0xff0), rs1 = rd = x2, funct3 = 0, opcode = OP-IMM.
        assert_eq!(addi.example_word, Some(0xff01_0113));
        let names: Vec<&str> = addi.fields.iter().map(|field| field.name).collect();
        assert_eq!(names, ["imm[11:0]", "rs1", "funct3", "rd", "opcode"]);
    }
}
