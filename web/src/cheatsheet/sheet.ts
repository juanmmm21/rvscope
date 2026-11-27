import type {
  CheatInstruction,
  CheatSheet,
  DirectiveInfo,
  PseudoInfo,
  RegisterInfo,
  SyscallInfo,
} from '../core';

// Secciones de la chuleta en el orden en que se leen: primero las instrucciones reales por
// grupo (las de la extensión M al final, como en la tabla), luego lo que el ensamblador añade
// encima (pseudoinstrucciones y directivas), las ecalls y los registros. El contenido es el
// que manda el núcleo; aquí solo se ordena y se pone nombre a cada sección.

export type SheetSection =
  | {
      readonly kind: 'instructions';
      readonly id: string;
      readonly title: string;
      readonly entries: readonly CheatInstruction[];
    }
  | {
      readonly kind: 'pseudo';
      readonly id: string;
      readonly title: string;
      readonly entries: readonly PseudoInfo[];
    }
  | {
      readonly kind: 'directives';
      readonly id: string;
      readonly title: string;
      readonly entries: readonly DirectiveInfo[];
    }
  | {
      readonly kind: 'syscalls';
      readonly id: string;
      readonly title: string;
      readonly entries: readonly SyscallInfo[];
    }
  | {
      readonly kind: 'registers';
      readonly id: string;
      readonly title: string;
      readonly entries: readonly RegisterInfo[];
    };

// Prefijo de los `id` del DOM, para no chocar con los del resto de la página.
const ID_PREFIX = 'cs-';

export function sheetSections(sheet: CheatSheet): SheetSection[] {
  const groups: SheetSection[] = sheet.groups.map((group) => ({
    kind: 'instructions',
    id: `${ID_PREFIX}${group.id}`,
    title: group.title,
    entries: group.instructions,
  }));
  return [
    ...groups,
    {
      kind: 'pseudo',
      id: `${ID_PREFIX}pseudo`,
      title: 'Pseudoinstrucciones',
      entries: sheet.pseudo_instructions,
    },
    {
      kind: 'directives',
      id: `${ID_PREFIX}directives`,
      title: 'Directivas',
      entries: sheet.directives,
    },
    {
      kind: 'syscalls',
      id: `${ID_PREFIX}syscalls`,
      title: 'Llamadas al sistema (ecall)',
      entries: sheet.syscalls,
    },
    {
      kind: 'registers',
      id: `${ID_PREFIX}registers`,
      title: 'Registros y convenio ABI',
      entries: sheet.registers,
    },
  ];
}

// Quién conserva el registro a través de una llamada, como se dice en clase.
export function saverLabel(saver: RegisterInfo['saver']): string {
  switch (saver) {
    case 'caller':
      return 'lo guarda quien llama';
    case 'callee':
      return 'lo guarda la función';
    case 'none':
      return '—';
  }
}

// Una ecall en una línea: número en a7 y qué entra y sale por los registros.
export function syscallSignature(info: SyscallInfo): string {
  const parts = [`a7 = ${String(info.number)}`];
  if (info.arguments !== '') {
    parts.push(info.arguments);
  }
  if (info.result !== '') {
    parts.push(`→ ${info.result}`);
  }
  return parts.join(' · ');
}

// Las expansiones con alternativas vienen separadas por « · o bien · »: cada alternativa en su
// línea se lee mejor que una sola línea larga.
export function expansionLines(pseudo: PseudoInfo): string[] {
  return pseudo.expansion.split(/\s+·\s+o bien\s+·\s+/u);
}
