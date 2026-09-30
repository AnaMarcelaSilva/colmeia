#!/usr/bin/env bash
# Mede processador e memória de um app e de todos os processos filhos dele
# (no Tauri, a tela web roda em processos WebKit filhos do app).
# Uso: ./medir.sh <nome-do-processo> [segundos]
#   ./medir.sh tela_flutter
#   ./medir.sh prototipo-react
set -euo pipefail
nome=${1:?informe o nome do processo}
segundos=${2:-10}
raiz=$(pgrep -x "$nome" | head -1) || { echo "processo $nome não encontrado"; exit 1; }

arvore() { echo "$1"; for f in $(pgrep -P "$1"); do arvore "$f"; done; }
tempo_cpu() { local t=0; for p in $(arvore "$raiz"); do [ -r /proc/$p/stat ] && t=$((t + $(awk '{print $14+$15}' /proc/$p/stat))); done; echo $t; }
memoria() { local m=0; for p in $(arvore "$raiz"); do m=$((m + $(awk '/^Pss:/ {print $2}' /proc/$p/smaps_rollup 2>/dev/null || echo 0))); done; echo $((m / 1024)); }

hz=$(getconf CLK_TCK)
antes=$(tempo_cpu); sleep "$segundos"; depois=$(tempo_cpu)
echo "$nome: processador $(( (depois - antes) * 100 / (hz * segundos) ))% (100% = um núcleo), memória $(memoria) MB (PSS)"
