#!/usr/bin/env bash
# Mede processador e memória de um app e de todos os processos filhos dele
# (no Tauri, a tela web roda em processos WebKit filhos do app).
# Uso: ./medir.sh <nome-do-processo> [segundos]
#      ./medir.sh --pid <pid> [segundos]
#   ./medir.sh tela_flutter
#   ./medir.sh --pid 12345 30
# Com --pid mede exatamente aquele processo: use quando houver mais de um com
# o mesmo nome (uma Colmeia de teste ao lado da de verdade, por exemplo).
set -euo pipefail
if [ "${1:-}" = "--pid" ]; then
	raiz=${2:?informe o pid}
	segundos=${3:-10}
	[ -d "/proc/$raiz" ] || { echo "processo $raiz não encontrado"; exit 1; }
	nome="$(cat "/proc/$raiz/comm") (pid $raiz)"
else
	nome=${1:?informe o nome do processo ou --pid <pid>}
	segundos=${2:-10}
	raiz=$(pgrep -x "$nome" | head -1) || { echo "processo $nome não encontrado"; exit 1; }
fi

arvore() { echo "$1"; for f in $(pgrep -P "$1"); do arvore "$f"; done; }
tempo_cpu() { local t=0; for p in $(arvore "$raiz"); do [ -r /proc/$p/stat ] && t=$((t + $(awk '{print $14+$15}' /proc/$p/stat))); done; echo $t; }
memoria() { local m=0; for p in $(arvore "$raiz"); do m=$((m + $(awk '/^Pss:/ {print $2}' /proc/$p/smaps_rollup 2>/dev/null || echo 0))); done; echo $((m / 1024)); }

hz=$(getconf CLK_TCK)
antes=$(tempo_cpu); sleep "$segundos"; depois=$(tempo_cpu)
# Uma casa decimal: com a tela parada, a diferença entre 0% e 0,4% importa.
echo "$nome: processador $(awk -v t=$((depois - antes)) -v hz="$hz" -v s="$segundos" 'BEGIN{printf "%.1f", t*100/(hz*s)}')% (100% = um núcleo), memória $(memoria) MB (PSS)"
