// Cómo se enseña un valor de 32 bits: registros y celdas de memoria comparten estas reglas.

export type ValueFormat = 'hex' | 'signed' | 'unsigned' | 'ascii';

export interface FormatOption {
  readonly id: ValueFormat;
  readonly label: string;
  readonly description: string;
}

export const VALUE_FORMATS: readonly FormatOption[] = [
  { id: 'hex', label: 'Hex', description: 'Hexadecimal' },
  { id: 'signed', label: 'Dec', description: 'Decimal con signo' },
  { id: 'unsigned', label: 'Sin signo', description: 'Decimal sin signo' },
  { id: 'ascii', label: 'ASCII', description: 'Caracteres ASCII' },
];

export function isValueFormat(value: string): value is ValueFormat {
  return VALUE_FORMATS.some((option) => option.id === value);
}

// Lo que no es ASCII imprimible se enseña con un punto medio, que no se confunde con un `.`.
const NON_PRINTABLE = '·';

export function asciiChar(byte: number): string {
  return byte >= 0x20 && byte < 0x7f ? String.fromCharCode(byte) : NON_PRINTABLE;
}

export function hexWord(value: number): string {
  return `0x${(value >>> 0).toString(16).padStart(8, '0')}`;
}

export function hexByte(value: number): string {
  return (value & 0xff).toString(16).padStart(2, '0');
}

export function formatValue(value: number, format: ValueFormat): string {
  switch (format) {
    case 'hex':
      return hexWord(value);
    case 'signed':
      return String(value | 0);
    case 'unsigned':
      return String(value >>> 0);
    case 'ascii': {
      // En el orden en que quedan en memoria (little-endian): un `lw` de la cadena "hola" se
      // lee «hola», igual que en la vista de memoria.
      let text = '';
      for (let shift = 0; shift < 32; shift += 8) {
        text += asciiChar((value >>> shift) & 0xff);
      }
      return text;
    }
  }
}
