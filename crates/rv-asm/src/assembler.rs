//! Ensamblado en dos pasadas: la primera sitúa cada sentencia y define las etiquetas; la
//! segunda evalúa operandos, expande pseudoinstrucciones y codifica.

use std::collections::{BTreeMap, HashMap};

use rv_isa::Reg;

use crate::error::{AsmError, LineError, LineResult, Span};
use crate::eval::{Context, Pass, SymbolDef, SymbolValue, is_local_key, local_key};
use crate::expand::expand;
use crate::parser::{Expr, LabelName, Operand, OperandKind, Statement, parse_line};
use crate::program::{Layout, ListingEntry, Program, Segment};

/// Tope por segmento, para que un `.space` desmesurado no agote la memoria del navegador.
const MAX_SEGMENT: u64 = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Section {
    Text,
    Data,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DataKind {
    Byte,
    Half,
    Word,
    /// Cadena, con terminador nulo o sin él.
    Str {
        terminated: bool,
    },
    Space,
}

impl DataKind {
    fn from_directive(name: &str) -> Option<DataKind> {
        Some(match name {
            ".byte" => DataKind::Byte,
            ".half" | ".short" => DataKind::Half,
            ".word" | ".long" => DataKind::Word,
            ".string" | ".asciz" => DataKind::Str { terminated: true },
            ".ascii" => DataKind::Str { terminated: false },
            ".space" | ".zero" => DataKind::Space,
            _ => return None,
        })
    }

    /// Alineamiento natural. Se aplica solo, como hacen los simuladores de clase, para que una
    /// etiqueta en la línea anterior a un `.word` no quede desalineada.
    fn alignment(self) -> u64 {
        match self {
            DataKind::Half => 2,
            DataKind::Word => 4,
            DataKind::Byte | DataKind::Str { .. } | DataKind::Space => 1,
        }
    }
}

#[derive(Clone, Debug)]
enum ItemKind {
    Instruction,
    Data(DataKind),
}

#[derive(Clone, Debug)]
struct Item {
    line: usize,
    section: Section,
    address: u32,
    size: u64,
    statement: Statement,
    kind: ItemKind,
}

struct Assembler {
    layout: Layout,
    symbols: HashMap<String, SymbolDef>,
    items: Vec<Item>,
    errors: Vec<AsmError>,
    section: Section,
    /// Desplazamiento actual dentro de cada segmento.
    text_size: u64,
    data_size: u64,
    /// Etiquetas (nombre en la tabla, tramo y línea) que esperan a la siguiente sentencia que
    /// ocupe memoria.
    pending: Vec<(String, Span, usize)>,
    /// Cuántas veces se ha definido ya cada etiqueta local numérica.
    local_counts: HashMap<u32, usize>,
}

fn align_up(value: u64, alignment: u64) -> u64 {
    value.div_ceil(alignment) * alignment
}

fn expr_operand(operand: &Operand) -> LineResult<&Expr> {
    match &operand.kind {
        OperandKind::Expr(expr) => Ok(expr),
        _ => Err(LineError::new(operand.span, "se esperaba un valor")),
    }
}

fn symbol_name(operand: &Operand) -> LineResult<&str> {
    match &operand.kind {
        OperandKind::Expr(Expr::Symbol(name, _)) => Ok(name),
        _ => Err(LineError::new(operand.span, "se esperaba un nombre")),
    }
}

impl Assembler {
    fn new(layout: Layout) -> Assembler {
        Assembler {
            layout,
            symbols: HashMap::new(),
            items: Vec::new(),
            errors: Vec::new(),
            section: Section::Text,
            text_size: 0,
            data_size: 0,
            pending: Vec::new(),
            local_counts: HashMap::new(),
        }
    }

    fn size_mut(&mut self) -> &mut u64 {
        match self.section {
            Section::Text => &mut self.text_size,
            Section::Data => &mut self.data_size,
        }
    }

    fn base(&self, section: Section) -> u32 {
        match section {
            Section::Text => self.layout.text_base,
            Section::Data => self.layout.data_base,
        }
    }

    fn current_address(&self) -> u32 {
        let offset = match self.section {
            Section::Text => self.text_size,
            Section::Data => self.data_size,
        };
        // El tope de segmento garantiza que el desplazamiento cabe en 32 bits.
        self.base(self.section).wrapping_add(offset as u32)
    }

    fn define(&mut self, name: &str, span: Span, line: usize, value: SymbolValue) {
        if Reg::parse(name).is_some() {
            self.errors.push(AsmError::at(
                line,
                span,
                format!("«{name}» es un registro y no puede usarse como nombre"),
            ));
            return;
        }
        if let Some(previous) = self.symbols.get(name) {
            self.errors.push(AsmError::at(
                line,
                span,
                format!("«{name}» ya está definido en la línea {}", previous.line),
            ));
            return;
        }
        self.symbols
            .insert(name.to_owned(), SymbolDef { value, line });
    }

    fn bind_pending(&mut self) {
        let address = self.current_address();
        for (name, span, line) in std::mem::take(&mut self.pending) {
            self.define(&name, span, line, SymbolValue::Label(address));
        }
    }

    /// Reserva `size` bytes alineados en la sección actual y devuelve su dirección.
    fn reserve(&mut self, alignment: u64, size: u64, line: usize, span: Span) -> Option<u32> {
        let aligned = align_up(*self.size_mut(), alignment);
        *self.size_mut() = aligned;
        self.bind_pending();
        let address = self.current_address();
        let end = aligned + size;
        if end > MAX_SEGMENT {
            self.errors
                .push(AsmError::at(line, span, "el segmento supera los 16 MiB"));
            return None;
        }
        *self.size_mut() = end;
        Some(address)
    }

    fn context(&self, pass: Pass) -> Context<'_> {
        Context {
            symbols: &self.symbols,
            pass,
            here: self.current_address(),
        }
    }

    // --- Primera pasada ---

    fn layout_line(&mut self, line: usize, text: &str) {
        let parsed = match parse_line(text) {
            Ok(parsed) => parsed,
            Err(error) => {
                self.errors.push(error.on_line(line));
                return;
            }
        };
        for part in parsed {
            for label in part.labels {
                let name = match label.name {
                    LabelName::Named(name) => name,
                    LabelName::Local(number) => {
                        let count = self.local_counts.entry(number).or_default();
                        *count += 1;
                        local_key(number, *count)
                    }
                };
                self.pending.push((name, label.span, line));
            }
            let Some(mut statement) = part.statement else {
                continue;
            };
            let result = match self.resolve_locals(&mut statement) {
                Ok(()) if statement.is_directive() => self.layout_directive(line, statement),
                Ok(()) => self.layout_instruction(line, statement),
                Err(error) => {
                    if !statement.is_directive() {
                        // Igual que con cualquier instrucción errónea: ocupa su sitio.
                        self.reserve(4, 4, line, statement.span());
                    }
                    Err(error)
                }
            };
            if let Err(error) = result {
                self.errors.push(error.on_line(line));
            }
        }
    }

    /// Fija a qué definición apunta cada `Nb`/`Nf` según lo visto hasta aquí del fuente.
    fn resolve_locals(&self, statement: &mut Statement) -> LineResult<()> {
        let mut error = None;
        statement.locals_mut(&mut |local| {
            let defined = self.local_counts.get(&local.number).copied().unwrap_or(0);
            if local.forward {
                local.ordinal = defined + 1;
            } else if defined == 0 {
                error.get_or_insert_with(|| {
                    LineError::new(
                        local.span,
                        format!(
                            "«{}»: no hay ninguna etiqueta «{}:» antes de esta línea",
                            local.text(),
                            local.number
                        ),
                    )
                });
            } else {
                local.ordinal = defined;
            }
        });
        error.map_or(Ok(()), Err)
    }

    fn layout_instruction(&mut self, line: usize, statement: Statement) -> LineResult<()> {
        if self.section == Section::Data {
            return Err(LineError::new(
                statement.name_span,
                "las instrucciones van en la sección .text",
            ));
        }
        let address = self.current_address();
        let expansion = match expand(&statement, address, &self.context(Pass::Layout)) {
            Ok(expansion) => expansion,
            Err(error) => {
                // Se reserva una instrucción para que las direcciones de lo que sigue no se
                // desplacen y la segunda pasada pueda seguir señalando errores con sentido.
                self.reserve(4, 4, line, statement.span());
                return Err(error);
            }
        };
        let size = 4 * expansion.emitted.len() as u64;
        if let Some(address) = self.reserve(4, size, line, statement.span()) {
            self.items.push(Item {
                line,
                section: Section::Text,
                address,
                size,
                statement,
                kind: ItemKind::Instruction,
            });
        }
        Ok(())
    }

    fn layout_directive(&mut self, line: usize, statement: Statement) -> LineResult<()> {
        let name = statement.name.as_str();
        let operands = &statement.operands;
        let arity_error =
            |usage: &str| LineError::new(statement.span(), format!("se usa así: {usage}"));
        match name {
            ".text" | ".data" | ".rodata" | ".bss" => {
                if !operands.is_empty() {
                    return Err(arity_error(name));
                }
                self.switch_section(name);
            }
            ".section" => {
                let [section] = operands.as_slice() else {
                    return Err(arity_error(".section .text  o  .section .data"));
                };
                let section_name = symbol_name(section)?;
                match section_name {
                    ".text" | ".data" | ".rodata" | ".bss" => self.switch_section(section_name),
                    _ => {
                        return Err(LineError::new(
                            section.span,
                            format!("sección desconocida «{section_name}»"),
                        ));
                    }
                }
            }
            ".option" => {
                // Lo emiten los compiladores (`.option nopic`); en rvscope no cambia nada
                // porque no hay instrucciones comprimidas ni código reubicable.
                let [option] = operands.as_slice() else {
                    return Err(arity_error(".option nombre"));
                };
                let option_name = symbol_name(option)?;
                if !matches!(
                    option_name,
                    "push" | "pop" | "rvc" | "norvc" | "pic" | "nopic" | "relax" | "norelax"
                ) {
                    return Err(LineError::new(
                        option.span,
                        format!("opción desconocida «{option_name}»"),
                    ));
                }
            }
            ".globl" | ".global" => {
                if operands.is_empty() {
                    return Err(arity_error(".globl símbolo"));
                }
                for operand in operands {
                    symbol_name(operand)?;
                }
            }
            ".equ" | ".set" => {
                let [target, value] = operands.as_slice() else {
                    return Err(arity_error(".equ NOMBRE, valor"));
                };
                let symbol = symbol_name(target)?.to_owned();
                let value = self.context(Pass::Layout).constant(expr_operand(value)?)?;
                self.define(&symbol, target.span, line, SymbolValue::Constant(value));
            }
            ".align" | ".p2align" | ".balign" => {
                let [amount] = operands.as_slice() else {
                    return Err(arity_error(&format!("{name} n")));
                };
                let value = self.context(Pass::Layout).constant(expr_operand(amount)?)?;
                let alignment = if name == ".balign" {
                    if !(1..=4096).contains(&value) || value & (value - 1) != 0 {
                        return Err(LineError::new(
                            amount.span,
                            ".balign necesita una potencia de dos entre 1 y 4096",
                        ));
                    }
                    value
                } else {
                    if !(0..=12).contains(&value) {
                        return Err(LineError::new(
                            amount.span,
                            format!("{name} va de 0 a 12 (alinea a 2^n bytes)"),
                        ));
                    }
                    1 << value
                };
                let alignment = u64::try_from(alignment).unwrap_or(1);
                self.reserve(alignment, 0, line, statement.span());
            }
            _ => {
                let Some(kind) = DataKind::from_directive(name) else {
                    return Err(LineError::new(
                        statement.name_span,
                        format!("directiva desconocida «{name}»"),
                    ));
                };
                let size = self.data_size_of(kind, &statement)?;
                if let Some(address) = self.reserve(kind.alignment(), size, line, statement.span())
                {
                    self.items.push(Item {
                        line,
                        section: self.section,
                        address,
                        size,
                        statement,
                        kind: ItemKind::Data(kind),
                    });
                }
            }
        }
        Ok(())
    }

    fn switch_section(&mut self, name: &str) {
        // Las etiquetas sueltas al final de una sección marcan su final, no el de la siguiente.
        self.bind_pending();
        self.section = if name == ".text" {
            Section::Text
        } else {
            Section::Data
        };
    }

    fn data_size_of(&self, kind: DataKind, statement: &Statement) -> LineResult<u64> {
        let operands = &statement.operands;
        if operands.is_empty() {
            return Err(LineError::new(
                statement.span(),
                format!("{} necesita al menos un valor", statement.name),
            ));
        }
        let count = operands.len() as u64;
        match kind {
            DataKind::Byte => Ok(count),
            DataKind::Half => Ok(2 * count),
            DataKind::Word => Ok(4 * count),
            DataKind::Str { terminated } => {
                operands
                    .iter()
                    .try_fold(0u64, |total, operand| match &operand.kind {
                        OperandKind::Str(text) => {
                            Ok(total + text.len() as u64 + u64::from(terminated))
                        }
                        _ => Err(LineError::new(
                            operand.span,
                            "se esperaba una cadena entre comillas",
                        )),
                    })
            }
            DataKind::Space => {
                let [amount] = operands.as_slice() else {
                    return Err(LineError::new(statement.span(), "se usa así: .space bytes"));
                };
                let value = self.context(Pass::Layout).constant(expr_operand(amount)?)?;
                u64::try_from(value)
                    .ok()
                    .filter(|size| *size <= MAX_SEGMENT)
                    .ok_or_else(|| {
                        LineError::new(amount.span, format!("tamaño no válido: {value}"))
                    })
            }
        }
    }

    // --- Segunda pasada ---

    fn emit_item(
        &self,
        item: &Item,
        segments: &mut Segments,
        listing: &mut Vec<ListingEntry>,
    ) -> LineResult<()> {
        let context = self.context(Pass::Final);
        let mut bytes = Vec::with_capacity(usize::try_from(item.size).unwrap_or(0));
        match item.kind {
            ItemKind::Instruction => {
                let expansion = expand(&item.statement, item.address, &context)?;
                for (index, emitted) in expansion.emitted.iter().enumerate() {
                    let word = emitted
                        .instruction
                        .encode()
                        .map_err(|error| LineError::new(emitted.span, error.to_string()))?;
                    bytes.extend_from_slice(&word.to_le_bytes());
                    listing.push(ListingEntry {
                        address: item.address.wrapping_add(4 * index as u32),
                        word,
                        instruction: emitted.instruction,
                        line: item.line,
                        pseudo: expansion.pseudo,
                    });
                }
            }
            ItemKind::Data(kind) => {
                for operand in &item.statement.operands {
                    match (kind, &operand.kind) {
                        (DataKind::Str { terminated }, OperandKind::Str(text)) => {
                            bytes.extend_from_slice(text.as_bytes());
                            if terminated {
                                bytes.push(0);
                            }
                        }
                        (DataKind::Space, _) => {
                            bytes.resize(bytes.len() + usize::try_from(item.size).unwrap_or(0), 0)
                        }
                        (DataKind::Byte | DataKind::Half | DataKind::Word, _) => {
                            let value = context.value(expr_operand(operand)?)?;
                            let (width, min, max) = match kind {
                                DataKind::Byte => (1, -128, 255),
                                DataKind::Half => (2, -32768, 65535),
                                _ => (4, i64::from(i32::MIN), i64::from(u32::MAX)),
                            };
                            if !(min..=max).contains(&value) {
                                return Err(LineError::new(
                                    operand.span,
                                    format!("{value} no cabe en {width} byte(s)"),
                                ));
                            }
                            bytes.extend_from_slice(&value.to_le_bytes()[..width]);
                        }
                        (DataKind::Str { .. }, _) => {
                            return Err(LineError::new(
                                operand.span,
                                "se esperaba una cadena entre comillas",
                            ));
                        }
                    }
                }
            }
        }
        segments.write(item.section, item.address, &bytes);
        Ok(())
    }
}

struct Segments {
    text: Segment,
    data: Segment,
}

impl Segments {
    fn write(&mut self, section: Section, address: u32, bytes: &[u8]) {
        let segment = match section {
            Section::Text => &mut self.text,
            Section::Data => &mut self.data,
        };
        let offset = address.wrapping_sub(segment.base) as usize;
        if let Some(slot) = segment.bytes.get_mut(offset..offset + bytes.len()) {
            slot.copy_from_slice(bytes);
        }
    }
}

/// Relleno del segmento de texto: los huecos que deja `.align` entre instrucciones se
/// ejecutan, así que van con `nop` (como en gas) y no con ceros, que son una instrucción ilegal.
fn nop_fill(base: u32, size: u64) -> Vec<u8> {
    const NOP: [u8; 4] = 0x0000_0013u32.to_le_bytes();
    (0..size)
        .map(|offset| NOP[((u64::from(base) + offset) % 4) as usize])
        .collect()
}

/// Ensambla con las direcciones por defecto.
pub fn assemble(source: &str) -> Result<Program, Vec<AsmError>> {
    assemble_with(source, Layout::default())
}

/// Ensambla `source`. Si hay errores se devuelven todos, ordenados por línea.
pub fn assemble_with(source: &str, layout: Layout) -> Result<Program, Vec<AsmError>> {
    let mut assembler = Assembler::new(layout);
    for (index, text) in source.lines().enumerate() {
        let line = index + 1;
        assembler.layout_line(line, text);
    }
    assembler.bind_pending();

    let text_end = u64::from(layout.text_base) + assembler.text_size;
    let data_end = u64::from(layout.data_base) + assembler.data_size;
    let overlaps = u64::from(layout.text_base) < data_end && u64::from(layout.data_base) < text_end;
    if overlaps || text_end > 1 << 32 || data_end > 1 << 32 {
        let last_line = source.lines().count().max(1);
        assembler.errors.push(AsmError::at(
            last_line,
            Span::default(),
            "el programa no cabe: los segmentos .text y .data se solapan o se salen de la memoria",
        ));
    }

    let mut segments = Segments {
        text: Segment {
            base: layout.text_base,
            bytes: nop_fill(layout.text_base, assembler.text_size),
        },
        data: Segment {
            base: layout.data_base,
            bytes: vec![0; usize::try_from(assembler.data_size).unwrap_or(0)],
        },
    };
    let mut listing = Vec::new();
    for item in &assembler.items {
        if let Err(error) = assembler.emit_item(item, &mut segments, &mut listing) {
            assembler.errors.push(error.on_line(item.line));
        }
    }

    if !assembler.errors.is_empty() {
        let mut errors = assembler.errors;
        errors.sort_by_key(|error| (error.line, error.column));
        return Err(errors);
    }

    let labels: BTreeMap<String, u32> = assembler
        .symbols
        .iter()
        .filter(|(name, _)| !is_local_key(name))
        .filter_map(|(name, def)| match def.value {
            SymbolValue::Label(address) => Some((name.clone(), address)),
            SymbolValue::Constant(_) => None,
        })
        .collect();
    Ok(Program {
        layout,
        text: segments.text,
        data: segments.data,
        entry: layout.text_base,
        labels,
        listing,
    })
}
