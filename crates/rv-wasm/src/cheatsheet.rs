//! Contenido de la chuleta de instrucciones: todo sale de las tablas que usan el ensamblador y el
//! simulador (`rv-isa`, `rv-asm` y las ecalls de `rv-core`), así que la chuleta no puede anunciar
//! nada que rvscope no admita ni olvidarse de algo que sí admite.

use rv_asm::{DIRECTIVES, DirectiveSpec};
use rv_core::{SYSCALLS, SyscallInfo};
use rv_isa::{
    Extension, Format, Group, INSTRUCTIONS, PSEUDO_INSTRUCTIONS, PseudoSpec, REGISTERS,
    RegisterInfo,
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
                .map(|spec| CheatInstruction {
                    name: spec.name,
                    syntax: spec.syntax(),
                    summary: spec.summary,
                    operation: spec.operation,
                    format: spec.format,
                    extension: spec.extension,
                })
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
}
