# Arquivo: protótipos da escolha da tela

Estes são os protótipos comparados na [decisão 0001](../docs/decisoes/0001-tela-em-rust-e-egui.md). Ficam aqui como histórico; não são mantidos.

| Pasta | O que é |
| --- | --- |
| `tela-react/` | React + xterm.js, empacotado com Tauri 2 (`src-tauri/`) |
| `tela-flutter/` | Flutter + xterm.dart, desktop Linux e Windows |

Os dois foram escritos para o núcleo dos protótipos, que escutava em `127.0.0.1:7777` sem autenticação. O núcleo atual não aceita mais essa conexão (veja a [decisão 0002](../docs/decisoes/0002-canal-local-sem-rede.md)).

## O que eles ensinaram

- **A frequência de atualização pesa mais que a tecnologia.** Atualizar os terminais 4 vezes por segundo em vez de 60 cortou o processador de 3 a 5 vezes nas três telas. Daí veio a regra de só o terminal em foco ser tempo real.
- **Controle de fluxo é obrigatório.** Sem ele, a carga pesada estourava a memória da tela React.
- **Histórico com limite.** O Flutter travou por mais de 20 s ao receber 5 MB de histórico de uma vez; o núcleo passou a reenviar só os últimos 128 KB.
- **No Linux, o WebKitGTK limita o Tauri.** Desligar o renderizador DMA-BUF ou o modo de composição ajudou só de 10 a 20%.
