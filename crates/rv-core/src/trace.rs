//! Historia reciente del pipeline como tabla instrucción × ciclo: cada fila es una instrucción
//! dinámica (la misma dirección dentro de un bucle da una fila por vuelta) y cada celda, la
//! etapa en que estaba en un ciclo, si esperaba o se vació y qué operandos le llegaron
//! adelantados.
//!
//! Las vistas no numeran las instrucciones, pero no hace falta: los modelos son en orden, así
//! que leídas de la última etapa a la primera (y en cada etapa de la vía 0 a la 1) las
//! instrucciones en vuelo salen siempre de la más antigua a la más joven. Las que siguen en
//! vuelo tras un ciclo son las de antes salvo las retiradas y las vaciadas, en el mismo orden,
//! y detrás solo pueden venir las recién buscadas.

use std::collections::VecDeque;

use rv_isa::Reg;
use serde::Serialize;

use crate::pipeline::Bypass;
use crate::view::{PipelineView, Slot, SlotState, Stage};

/// Operando que llegó adelantado a EX.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct TraceBypass {
    pub reg: Reg,
    pub from: Bypass,
    pub lane: u8,
}

/// Dónde estaba una instrucción en un ciclo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct TraceCell {
    pub cycle: u64,
    pub stage: Stage,
    pub lane: u8,
    pub state: SlotState,
    /// `rs1` y `rs2`, solo en EX y solo si llegaron adelantados.
    pub bypasses: [Option<TraceBypass>; 2],
    /// Cierto si la instrucción lleva una excepción o se para en este ciclo.
    pub trap: bool,
}

/// Una instrucción dinámica con las celdas de los ciclos que ha pasado dentro de la ventana,
/// consecutivas: una instrucción no sale del pipeline y vuelve.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TraceRow {
    /// Orden de búsqueda; crece con cada instrucción nueva.
    pub id: u64,
    pub pc: u32,
    pub word: Option<u32>,
    pub cells: Vec<TraceCell>,
}

/// Las últimas `window` columnas de la tabla del pipeline, alimentadas con la vista de cada
/// ciclo en orden.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PipelineTrace {
    window: u64,
    next_id: u64,
    /// Último ciclo apuntado; 0 si aún no hay ninguno.
    cycle: u64,
    rows: VecDeque<TraceRow>,
    /// Filas de las instrucciones que siguen en vuelo, de la más antigua a la más joven.
    in_flight: Vec<u64>,
}

impl PipelineTrace {
    /// `window` es cuántos ciclos se guardan como mucho (al menos 1).
    pub fn new(window: u64) -> PipelineTrace {
        PipelineTrace {
            window: window.max(1),
            next_id: 0,
            cycle: 0,
            rows: VecDeque::new(),
            in_flight: Vec::new(),
        }
    }

    pub fn window(&self) -> u64 {
        self.window
    }

    /// Último ciclo apuntado.
    pub fn cycle(&self) -> u64 {
        self.cycle
    }

    /// Primer ciclo que queda en la ventana (el último si solo hay uno); 0 si no hay ninguno.
    pub fn first_cycle(&self) -> u64 {
        if self.cycle == 0 {
            0
        } else {
            self.cycle.saturating_sub(self.window - 1).max(1)
        }
    }

    /// Filas en orden de búsqueda.
    pub fn rows(&self) -> impl Iterator<Item = &TraceRow> {
        self.rows.iter()
    }

    /// Apunta la vista de un ciclo; `cycle` tiene que ser posterior al último apuntado. Se
    /// numera aparte de `view.cycle` porque el ciclo en que un modelo se para sin retirar nada
    /// no cuenta en las estadísticas pero sí ocupa una columna.
    pub fn record(&mut self, cycle: u64, view: &PipelineView) {
        if cycle <= self.cycle {
            return;
        }
        // Si se ha saltado algún ciclo, lo que estaba en vuelo ya no se puede seguir.
        if cycle != self.cycle + 1 {
            self.in_flight.clear();
        }
        self.cycle = cycle;
        let last_stage = view.stages.last().map(|stage| stage.stage);
        let mut continuing = self.in_flight.drain(..).collect::<VecDeque<u64>>();
        let mut still_in_flight = Vec::new();
        for (stage, lane, slot) in oldest_first(view) {
            // Por orden, la siguiente que sigue en vuelo; si no es la misma dirección, el
            // modelo ha hecho algo que esta lectura no prevé y se empieza fila nueva.
            let id = match continuing.pop_front() {
                Some(id) if self.row(id).is_some_and(|row| row.pc == slot.pc) => id,
                _ => {
                    continuing.clear();
                    self.start_row(slot)
                }
            };
            let cell = TraceCell {
                cycle,
                stage,
                lane: u8::try_from(lane).unwrap_or(u8::MAX),
                state: slot.state,
                bypasses: [slot.rs1, slot.rs2].map(|operand| {
                    operand.and_then(|operand| {
                        operand.bypass.map(|from| TraceBypass {
                            reg: operand.reg,
                            from,
                            lane: operand.bypass_lane.unwrap_or(0),
                        })
                    })
                }),
                trap: slot.trap.is_some(),
            };
            if let Some(row) = self.row_mut(id) {
                row.cells.push(cell);
            }
            if slot.state != SlotState::Flushed && Some(stage) != last_stage {
                still_in_flight.push(id);
            }
        }
        self.in_flight = still_in_flight;
        self.trim();
    }

    fn start_row(&mut self, slot: &Slot) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.rows.push_back(TraceRow {
            id,
            pc: slot.pc,
            word: slot.word,
            cells: Vec::new(),
        });
        id
    }

    fn row(&self, id: u64) -> Option<&TraceRow> {
        self.rows.iter().rev().find(|row| row.id == id)
    }

    fn row_mut(&mut self, id: u64) -> Option<&mut TraceRow> {
        self.rows.iter_mut().rev().find(|row| row.id == id)
    }

    /// Quita las celdas anteriores a la ventana y las filas que se quedan sin ninguna (no
    /// solo las primeras: una vaciada puede acabar antes que otra más antigua retenida).
    fn trim(&mut self) {
        let first = self.first_cycle();
        for row in &mut self.rows {
            let old = row.cells.partition_point(|cell| cell.cycle < first);
            row.cells.drain(..old);
        }
        self.rows.retain(|row| !row.cells.is_empty());
    }
}

/// Huecos ocupados de la última etapa a la primera y, en cada una, de la vía 0 a la 1: de la
/// instrucción más antigua a la más joven.
fn oldest_first(view: &PipelineView) -> impl Iterator<Item = (Stage, usize, &Slot)> {
    view.stages.iter().rev().flat_map(|stage| {
        stage
            .lanes
            .iter()
            .enumerate()
            .filter_map(move |(lane, slot)| slot.as_ref().map(|slot| (stage.stage, lane, slot)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::StageView;

    fn slot(pc: u32, state: SlotState) -> Option<Slot> {
        Some(Slot::fetched(pc, 0x0000_0013, state))
    }

    /// Vista de 5 etapas de una vía con `slots` de IF a WB.
    fn view(cycle: u64, slots: [Option<Slot>; 5]) -> PipelineView {
        PipelineView {
            cycle,
            stages: Stage::PIPELINE
                .iter()
                .zip(slots)
                .map(|(&stage, slot)| StageView {
                    stage,
                    lanes: vec![slot],
                })
                .collect(),
        }
    }

    fn stages(row: &TraceRow) -> Vec<(u64, Stage, SlotState)> {
        row.cells
            .iter()
            .map(|cell| (cell.cycle, cell.stage, cell.state))
            .collect()
    }

    use SlotState::{Active, Flushed, Stalled};

    #[test]
    fn a_stall_keeps_the_same_row_and_a_loop_gets_a_new_one() {
        let mut trace = PipelineTrace::new(16);
        trace.record(1, &view(1, [slot(0, Active), None, None, None, None]));
        trace.record(
            2,
            &view(2, [slot(4, Active), slot(0, Active), None, None, None]),
        );
        // ID retenida: 4 sigue en IF y 0 en ID.
        trace.record(
            3,
            &view(
                3,
                [
                    slot(8, Stalled),
                    slot(4, Stalled),
                    slot(0, Active),
                    None,
                    None,
                ],
            ),
        );
        trace.record(
            4,
            &view(
                4,
                [
                    slot(8, Active),
                    slot(4, Active),
                    None,
                    slot(0, Active),
                    None,
                ],
            ),
        );
        // Un salto en EX (el de 4) vacía IF e ID; la siguiente búsqueda vuelve a 0.
        trace.record(
            5,
            &view(
                5,
                [
                    slot(12, Flushed),
                    slot(8, Flushed),
                    slot(4, Active),
                    None,
                    slot(0, Active),
                ],
            ),
        );
        trace.record(
            6,
            &view(6, [slot(0, Active), None, None, slot(4, Active), None]),
        );
        let rows: Vec<&TraceRow> = trace.rows().collect();
        assert_eq!(
            rows.iter().map(|row| row.pc).collect::<Vec<_>>(),
            [0, 4, 8, 12, 0]
        );
        assert_eq!(
            stages(rows[0]),
            [
                (1, Stage::If, Active),
                (2, Stage::Id, Active),
                (3, Stage::Ex, Active),
                (4, Stage::Mem, Active),
                (5, Stage::Wb, Active),
            ]
        );
        assert_eq!(
            stages(rows[1]),
            [
                (2, Stage::If, Active),
                (3, Stage::Id, Stalled),
                (4, Stage::Id, Active),
                (5, Stage::Ex, Active),
                (6, Stage::Mem, Active),
            ]
        );
        assert_eq!(
            stages(rows[2]),
            [
                (3, Stage::If, Stalled),
                (4, Stage::If, Active),
                (5, Stage::Id, Flushed),
            ]
        );
        assert_eq!(stages(rows[3]), [(5, Stage::If, Flushed)]);
        assert_eq!(stages(rows[4]), [(6, Stage::If, Active)]);
        assert_eq!(rows[4].id, 4);
    }

    #[test]
    fn rows_that_leave_the_window_go_even_behind_an_older_one() {
        let mut trace = PipelineTrace::new(1);
        trace.record(1, &view(1, [slot(0, Active), None, None, None, None]));
        trace.record(
            2,
            &view(
                2,
                [
                    slot(8, Flushed),
                    slot(4, Flushed),
                    slot(0, Active),
                    None,
                    None,
                ],
            ),
        );
        trace.record(
            3,
            &view(3, [slot(16, Active), None, None, slot(0, Active), None]),
        );
        let rows: Vec<&TraceRow> = trace.rows().collect();
        assert_eq!(rows.len(), 2);
        assert_eq!(
            (rows[0].pc, stages(rows[0])),
            (0, vec![(3, Stage::Mem, Active)])
        );
        assert_eq!(
            (rows[1].pc, stages(rows[1])),
            (16, vec![(3, Stage::If, Active)])
        );
    }

    /// Pipeline sin riesgos que busca una instrucción por ciclo: la buscada en el ciclo `k`
    /// tiene `pc = 4k` y está en la etapa `c - k` en el ciclo `c`.
    fn flowing(cycle: u64) -> PipelineView {
        let slots = [0u64, 1, 2, 3, 4].map(|depth| {
            cycle
                .checked_sub(depth)
                .filter(|&fetched| fetched >= 1)
                .and_then(|fetched| slot(u32::try_from(fetched * 4).ok()?, Active))
        });
        view(cycle, slots)
    }

    #[test]
    fn only_the_last_cycles_are_kept() {
        let mut trace = PipelineTrace::new(3);
        assert_eq!((trace.first_cycle(), trace.cycle()), (0, 0));
        for cycle in 1..=6 {
            trace.record(cycle, &flowing(cycle));
        }
        assert_eq!((trace.first_cycle(), trace.cycle()), (4, 6));
        let rows: Vec<&TraceRow> = trace.rows().collect();
        assert_eq!(
            rows.iter().map(|row| row.pc).collect::<Vec<_>>(),
            [4, 8, 12, 16, 20, 24]
        );
        // La primera entró en el ciclo 1 y sale en el 5: solo quedan MEM y WB.
        assert_eq!(
            stages(rows[0]),
            [(4, Stage::Mem, Active), (5, Stage::Wb, Active)]
        );
        assert_eq!(rows[5].id, 5);
        assert!(
            rows.iter()
                .flat_map(|row| &row.cells)
                .all(|cell| cell.cycle >= 4)
        );
    }

    #[test]
    fn an_old_or_skipped_cycle_does_not_join_rows() {
        let mut trace = PipelineTrace::new(8);
        trace.record(1, &view(1, [slot(0, Active), None, None, None, None]));
        trace.record(1, &view(1, [slot(40, Active), None, None, None, None]));
        assert_eq!(trace.rows().count(), 1);
        trace.record(
            3,
            &view(3, [slot(8, Active), slot(4, Active), None, None, None]),
        );
        let pcs: Vec<u32> = trace.rows().map(|row| row.pc).collect();
        assert_eq!(pcs, [0, 4, 8]);
    }

    #[test]
    fn an_unexpected_address_starts_a_new_row() {
        let mut trace = PipelineTrace::new(8);
        trace.record(1, &view(1, [slot(0, Active), None, None, None, None]));
        trace.record(2, &view(2, [None, slot(12, Active), None, None, None]));
        let pcs: Vec<u32> = trace.rows().map(|row| row.pc).collect();
        assert_eq!(pcs, [0, 12]);
    }
}
