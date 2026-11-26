#!/usr/bin/env bash
# Regenera los riscv-tests preprocesados que usan los tests de conformidad de rv-core.
#
# Uso: scripts/vendor-riscv-tests.sh <ruta a un clon de riscv-software-src/riscv-tests>
#
# Los .S oficiales dependen del preprocesador de C (macros de test_macros.h). Se expanden aquí
# una vez con el cpp del sistema y se guarda el resultado, para que la CI y quien clone el
# repositorio no necesiten ni cpp ni una toolchain de RISC-V.
set -euo pipefail

if [[ $# -ne 1 || ! -d "$1/isa/macros/scalar" ]]; then
  echo "uso: $0 <clon de riscv-tests>" >&2
  exit 2
fi

upstream="$(cd "$1" && pwd)"
root="$(cd "$(dirname "$0")/.." && pwd)"
env_dir="$root/scripts/riscv-tests-env"
out="$root/crates/rv-core/tests/riscv-tests"
commit="$(git -C "$upstream" rev-parse HEAD)"

# fence_i necesita Zifencei y ma_data, accesos desalineados (rvscope los trata como excepción).
excluded=" fence_i ma_data "

# rv-asm no tiene .rept; los dos tests que lo usan solo repiten líneas sueltas.
expand_rept() {
  awk '
    $1 == ".rept" { times = $2; body = ""; inside = 1; next }
    $1 == ".endr" { for (i = 0; i < times; i++) printf "%s", body; inside = 0; next }
    inside { body = body $0 "\n"; next }
    { print }
  '
}

rm -rf "$out"
mkdir -p "$out"
cp "$upstream/LICENSE" "$out/LICENSE"

for suite in rv32ui rv32um; do
  mkdir -p "$out/$suite"
  for source in "$upstream/isa/$suite"/*.S; do
    name="$(basename "$source" .S)"
    if [[ "$excluded" == *" $name "* ]]; then
      continue
    fi
    {
      echo "# riscv-tests $suite/$name.S (commit $commit), preprocesado por"
      echo "# scripts/vendor-riscv-tests.sh. Licencia BSD: ver LICENSE en esta carpeta."
      cpp -x assembler-with-cpp -P -nostdinc -undef -D__riscv_xlen=32 \
        -I "$env_dir" -I "$upstream/isa/macros/scalar" -I "$upstream/isa/$suite" \
        "$source" | sed -e 's/[[:space:]]*$//' -e '/^[[:space:]]*#/d' -e '/^$/d' | expand_rept
    } > "$out/$suite/$name.s"
  done
done

echo "$commit" > "$out/UPSTREAM_COMMIT"
echo "riscv-tests de $commit en $out"
