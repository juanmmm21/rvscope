//! Catálogo de modelos de procesador y una máquina que puede ser cualquiera de ellos.
//!
//! `Machine` es un enum y no un `Box<dyn Processor>` porque la interfaz necesita clonarla para
//! guardar instantáneas del paso atrás, y un objeto de trait no se puede clonar.

use rv_asm::Program;
use serde::Serialize;

use crate::dual_issue::DualIssue;
use crate::five_stage::{FiveStage, Hazards};
use crate::map::MapError;
use crate::processor::{Halt, Processor, Retired, Stats};
use crate::single_cycle::SingleCycle;
use crate::state::ArchState;
use crate::syscall::Console;
use crate::view::PipelineView;

/// Modelos de procesador que se pueden elegir.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub enum Model {
    SingleCycle,
    FiveStageNoHazards,
    FiveStageStalls,
    FiveStageForwarding,
    DualIssue,
}

impl Model {
    /// Todos los modelos, en el orden en que se listan.
    pub const ALL: [Model; 5] = [
        Model::SingleCycle,
        Model::FiveStageNoHazards,
        Model::FiveStageStalls,
        Model::FiveStageForwarding,
        Model::DualIssue,
    ];

    /// Identificador estable: el de `--cpu` en la CLI y el que guarda la interfaz.
    pub fn id(self) -> &'static str {
        match self {
            Model::SingleCycle => "un-ciclo",
            Model::FiveStageNoHazards => "5-etapas-sin-riesgos",
            Model::FiveStageStalls => "5-etapas-burbujas",
            Model::FiveStageForwarding => "5-etapas-forwarding",
            Model::DualIssue => "2-vias",
        }
    }

    pub fn from_id(id: &str) -> Option<Model> {
        Model::ALL.into_iter().find(|model| model.id() == id)
    }

    /// Nombre largo para mostrar; es el mismo que da `Processor::name`.
    pub fn title(self) -> &'static str {
        match self {
            Model::SingleCycle => "Un ciclo",
            Model::FiveStageNoHazards => "5 etapas sin forwarding ni detección de riesgos",
            Model::FiveStageStalls => "5 etapas con detección de riesgos y burbujas",
            Model::FiveStageForwarding => "5 etapas con forwarding y detección de riesgos",
            Model::DualIssue => "doble emisión en orden (2 vías de 5 etapas con forwarding)",
        }
    }

    /// Nombre corto para el selector de la interfaz, donde no cabe el largo.
    pub fn label(self) -> &'static str {
        match self {
            Model::SingleCycle => "Un ciclo",
            Model::FiveStageNoHazards => "5 etapas · sin riesgos",
            Model::FiveStageStalls => "5 etapas · burbujas",
            Model::FiveStageForwarding => "5 etapas · forwarding",
            Model::DualIssue => "2 vías",
        }
    }

    /// Cierto si el modelo da el mismo resultado que el de un ciclo con cualquier programa; el
    /// que no detecta riesgos solo lo hace con programas sin dependencias cercanas.
    pub fn is_equivalent(self) -> bool {
        self != Model::FiveStageNoHazards
    }

    /// Procesador de este modelo con el programa cargado.
    pub fn boot(self, program: &Program) -> Result<Machine, MapError> {
        Ok(match self {
            Model::SingleCycle => Machine::SingleCycle(SingleCycle::new(program)?),
            Model::FiveStageNoHazards => {
                Machine::FiveStage(FiveStage::new(program, Hazards::Ignore)?)
            }
            Model::FiveStageStalls => Machine::FiveStage(FiveStage::new(program, Hazards::Stall)?),
            Model::FiveStageForwarding => {
                Machine::FiveStage(FiveStage::new(program, Hazards::Forward)?)
            }
            Model::DualIssue => Machine::DualIssue(DualIssue::new(program)?),
        })
    }
}

/// Cualquiera de los modelos, con el mismo contrato `Processor`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Machine {
    SingleCycle(SingleCycle),
    FiveStage(FiveStage),
    DualIssue(DualIssue),
}

impl Machine {
    fn inner(&self) -> &dyn Processor {
        match self {
            Machine::SingleCycle(cpu) => cpu,
            Machine::FiveStage(cpu) => cpu,
            Machine::DualIssue(cpu) => cpu,
        }
    }

    fn inner_mut(&mut self) -> &mut dyn Processor {
        match self {
            Machine::SingleCycle(cpu) => cpu,
            Machine::FiveStage(cpu) => cpu,
            Machine::DualIssue(cpu) => cpu,
        }
    }

    pub fn model(&self) -> Model {
        match self {
            Machine::SingleCycle(_) => Model::SingleCycle,
            Machine::FiveStage(cpu) => match cpu.hazards() {
                Hazards::Ignore => Model::FiveStageNoHazards,
                Hazards::Stall => Model::FiveStageStalls,
                Hazards::Forward => Model::FiveStageForwarding,
            },
            Machine::DualIssue(_) => Model::DualIssue,
        }
    }
}

impl Processor for Machine {
    fn name(&self) -> &'static str {
        self.inner().name()
    }

    fn state(&self) -> &ArchState {
        self.inner().state()
    }

    fn stats(&self) -> Stats {
        self.inner().stats()
    }

    fn halt(&self) -> Option<Halt> {
        self.inner().halt()
    }

    fn view(&self) -> &PipelineView {
        self.inner().view()
    }

    fn console_mut(&mut self) -> &mut Console {
        self.inner_mut().console_mut()
    }

    fn awaiting_input(&self) -> bool {
        self.inner().awaiting_input()
    }

    fn step(&mut self) -> Vec<Retired> {
        self.inner_mut().step()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_are_unique() {
        for model in Model::ALL {
            assert_eq!(Model::from_id(model.id()), Some(model));
        }
        assert_eq!(Model::from_id("6-etapas"), None);
    }

    #[test]
    fn labels_are_short_and_unique() {
        let labels: std::collections::HashSet<&str> = Model::ALL.map(Model::label).into();
        assert_eq!(labels.len(), Model::ALL.len());
        assert!(labels.iter().all(|label| label.chars().count() <= 24));
    }

    #[test]
    fn machines_report_their_model_and_title() {
        let program = rv_asm::assemble("li a7, 10\necall\n").unwrap();
        for model in Model::ALL {
            let mut machine = model.boot(&program).unwrap();
            assert_eq!(machine.model(), model);
            assert_eq!(machine.name(), model.title());
            machine.run(100);
            assert!(machine.halt().is_some());
        }
    }
}
