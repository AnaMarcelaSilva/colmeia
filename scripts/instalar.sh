#!/usr/bin/env bash
# Instala a Colmeia só para o seu usuário, sem sudo:
#   ~/.local/bin/colmeia e ~/.local/bin/colmeia-nucleo
#   o atalho em ~/.local/share/applications e o ícone em ~/.local/share/icons
# Roda de dentro da pasta da release (com os binários ao lado) ou da raiz do
# repositório depois de compilar (target/release/colmeia e bin/colmeia-nucleo).
set -euo pipefail
aqui=$(cd "$(dirname "$0")" && pwd)
raiz=$aqui
[ -f "$raiz/colmeia" ] || raiz=$(cd "$aqui/.." && pwd)

achar() { for c in "$@"; do [ -f "$c" ] && { echo "$c"; return; }; done; return 1; }

# Na raiz do repositório, um binário mais velho que o código instalaria uma
# versão anterior sem ninguém perceber: o núcleo é recompilado (é rápido) e a
# tela, que demora mais, só gera um aviso.
if [ -d "$raiz/nucleo" ]; then
	if command -v go >/dev/null; then
		echo "Compilando o núcleo…"
		go -C "$raiz/nucleo" build -o ../bin/colmeia-nucleo ./cmd/colmeia-nucleo
	elif [ -f "$raiz/bin/colmeia-nucleo" ] && [ -n "$(find "$raiz/nucleo" -name '*.go' -newer "$raiz/bin/colmeia-nucleo" -print -quit)" ]; then
		echo "Aviso: bin/colmeia-nucleo é mais velho que o código do núcleo e o Go não está instalado para recompilar."
	fi
	if [ -f "$raiz/target/release/colmeia" ] && [ -n "$(find "$raiz/app" "$raiz/mascote" -name '*.rs' -newer "$raiz/target/release/colmeia" -print -quit)" ]; then
		echo "Aviso: target/release/colmeia é mais velho que o código da tela. Rode 'cargo build --release' antes de instalar."
	fi
fi
tela=$(achar "$raiz/colmeia" "$raiz/target/release/colmeia") || { echo "não achei o binário colmeia (compile com: cargo build --release)"; exit 1; }
nucleo=$(achar "$raiz/colmeia-nucleo" "$raiz/bin/colmeia-nucleo") || { echo "não achei o colmeia-nucleo (compile com: go -C nucleo build -o ../bin/colmeia-nucleo ./cmd/colmeia-nucleo)"; exit 1; }
atalho=$(achar "$raiz/colmeia.desktop" "$raiz/packaging/colmeia.desktop")
icone=$(achar "$raiz/colmeia.svg" "$raiz/packaging/colmeia.svg")

bin="$HOME/.local/bin"
apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
icones="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/scalable/apps"
mkdir -p "$bin" "$apps" "$icones"
install -m 0755 "$tela" "$bin/colmeia"
install -m 0755 "$nucleo" "$bin/colmeia-nucleo"
# O atalho aponta para o caminho completo: funciona mesmo sem ~/.local/bin no PATH.
sed "s|^Exec=colmeia$|Exec=$bin/colmeia|" "$atalho" > "$apps/colmeia.desktop"
chmod 0644 "$apps/colmeia.desktop"
install -m 0644 "$icone" "$icones/colmeia.svg"
command -v update-desktop-database >/dev/null && update-desktop-database "$apps" 2>/dev/null || true

echo "Colmeia instalada em $bin."
case ":$PATH:" in
	*":$bin:"*) ;;
	*) echo "Para chamar pelo terminal, acrescente $bin ao PATH." ;;
esac
echo "Um núcleo que já estava rodando continua na versão anterior: rode 'colmeia-nucleo --encerrar' antes de abrir de novo."
