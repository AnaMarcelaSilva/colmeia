# 0001 · Tela em Rust + egui, núcleo em Go

**Decidido em 29/09/2026.**

## Contexto

A tela precisa mostrar vários terminais de agentes ao mesmo tempo, mais o quadro de tarefas, gastando pouco. Três protótipos da mesma tela (10 agentes, o mesmo núcleo em Go) foram comparados no Linux: React + xterm.js empacotado com Tauri, Flutter + xterm.dart e Rust com egui + alacritty_terminal. Os dois primeiros estão em [`arquivo/`](../../arquivo/).

## Medições (carga leve, 10 agentes, rodada com a máquina mais calma)

| Medida | React + Tauri | Flutter | Rust + egui |
| --- | --- | --- | --- |
| Processador, terminais em tempo real | 51% | 35% | 11% |
| Processador, terminais a cada 250 ms | 20% | 12% | 7% |
| Memória parado | 243 MB | 80 MB | 58 MB |
| Saída desenhada na carga pesada | 34 MB/s | 16 MB/s | 113 MB/s |

Processador em % de um núcleo. Em cinco rodadas alternadas a ordem foi sempre a mesma, embora os valores absolutos tenham variado até 3 vezes com a carga da máquina.

## Decisão

Tela em Rust com egui, terminal com alacritty_terminal (o emulador do Alacritty e do Zed), núcleo em Go separado.

## Consequências

- Mais leve em processador e memória, e o terminal mais maduro dos três.
- Componentes que a web tem prontos (quadro, diferenças de código, Markdown) precisam ser feitos à mão. O protótipo do quadro mostrou que é viável: 4 a 5% de processador com 500 cartões e 10 agentes ativos.
- O Tauri ficou limitado pelo WebKitGTK no Linux; o xterm.dart está pouco mantido (última versão em fevereiro de 2024).
- O celular, no futuro, pode usar outra tecnologia, porque só precisa falar o protocolo do núcleo.
