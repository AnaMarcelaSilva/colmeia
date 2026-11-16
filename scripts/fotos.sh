#!/bin/sh
# As fotos das telas do app (egui_kittest): desenha cada tela com dados
# fictícios e grava o PNG em app/tests/fotos/. Rodar depois de mexer numa
# tela e olhar as imagens das telas tocadas.
#
#   scripts/fotos.sh            grava as fotos de todas as telas
#   scripts/fotos.sh daily      só as que têm "daily" no nome
set -e
cd "$(dirname "$0")/.."
UPDATE_SNAPSHOTS=true cargo test -p colmeia --quiet fotos -- "$@" 2>&1 | grep -E "^test |test result|panicked|error" || true
echo "Fotos em:"
ls -1 app/tests/fotos/*.png 2>/dev/null | sed 's|^|  |'
