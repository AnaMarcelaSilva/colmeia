# Dependências e licenças

O que entra no binário do núcleo (`go version -m colmeia-nucleo`), conferido pelo arquivo de licença de cada módulo. Nenhuma é GPL, LGPL ou AGPL. A tela (Rust) não ganhou dependência nova na entrega dos bancos de dados.

| Módulo | Versão | Licença | Para quê |
| --- | --- | --- | --- |
| github.com/coder/websocket | v1.8.15 | ISC | WebSocket dos terminais e dos eventos |
| github.com/creack/pty | v1.1.24 | MIT | Pseudo-terminais dos agentes |
| modernc.org/sqlite (+ libc, mathutil, memory) | v1.60.1 | BSD-3-Clause | O banco da Colmeia e as conexões SQLite |
| github.com/dustin/go-humanize, github.com/google/uuid, github.com/remyoudompheng/bigfft | | MIT, BSD-3-Clause | Dependências do modernc |
| github.com/jackc/pgx/v5 (+ pgpassfile, pgservicefile, puddle) | v5.11.0 | MIT | PostgreSQL |
| github.com/go-sql-driver/mysql | v1.10.1 | MPL-2.0 | MySQL e MariaDB (usado como dependência, sem modificação) |
| filippo.io/edwards25519 | v1.2.0 | BSD-3-Clause | Dependência do driver MySQL |
| github.com/microsoft/go-mssqldb (+ golang-sql/civil, golang-sql/sqlexp, shopspring/decimal) | v1.11.2 | BSD-3-Clause, Apache-2.0, MIT | SQL Server (o subpacote `azuread` não é importado) |
| github.com/zalando/go-keyring | v0.2.8 | MIT | Chaveiro do sistema |
| github.com/godbus/dbus/v5 | v5.2.2 | BSD-2-Clause | Secret Service pelo D-Bus (dependência do go-keyring) |
| golang.org/x/crypto, x/sync, x/sys, x/text | | BSD-3-Clause | Biblioteca estendida do Go (no Windows, x/sys/windows: ConPTY, jobs e listas de acesso) |

## A abelha fixa (`colmeia-abelha`)

O programa da abelha fixa usa as mesmas bibliotecas da tela (eframe, tungstenite, serde) e o canal dela, agora no crate `canal`. Ele chama diretamente três que já vinham no binário da tela por dependência do eframe e do winit, sem nenhum crate de fora novo no `Cargo.lock`:

| Crate | Versão | Licença | Para quê |
| --- | --- | --- | --- |
| x11rb | 0.13 | MIT OR Apache-2.0 | Pedir ao gerenciador de janelas do X11 que a abelha fique em todas as áreas de trabalho e fora da barra de tarefas, e saber se há compositor (só no Linux) |
| raw-window-handle | 0.6 | MIT OR Apache-2.0 OR Zlib | O número da janela no X11 (só no Linux) |
| libc | 0.2 | MIT OR Apache-2.0 | `flock`: uma abelha só por perfil (só no Unix) |

## A voz (`colmeia-voz`)

O ditado, os comandos falados e a voz da abelha ficam num programa à parte ([decisão 0014](decisoes/0014-ditado-em-processo-a-parte.md)): nada disto entra no binário da tela (`cargo tree -p colmeia` não mostra nenhum deles). O núcleo baixa o modelo só com a biblioteca padrão do Go.

| Crate | Versão | Licença | Para quê |
| --- | --- | --- | --- |
| cpal | 0.18 | Apache-2.0 | Captura do microfone (PulseAudio ou ALSA no Linux, WASAPI no Windows) |
| pulseaudio | 0.3 | MIT | O protocolo do PulseAudio em Rust, sem a libpulse (dependência do cpal no Linux) |
| alsa, alsa-sys | 0.11, 0.4 | MIT OR Apache-2.0, MIT | ALSA, quando não há servidor de som (dependência do cpal) |
| whisper-rs, whisper-rs-sys | 0.16, 0.15 | Unlicense | Ligação com o whisper.cpp |
| whisper.cpp e ggml (dentro do whisper-rs-sys) | | MIT | A transcrição, compilada junto, só na CPU |
| dasp_sample, futures, num-derive, enum-primitive-derive, mio | | MIT OR Apache-2.0, MIT | Dependências do cpal e do pulseaudio |
| bindgen, cmake, clang-sys, fs_extra (só na compilação) | | BSD-3-Clause, MIT OR Apache-2.0, Apache-2.0, MIT | Compilar o whisper.cpp e gerar a ligação |

O Unlicense (whisper-rs) põe o código em domínio público, compatível com MIT e Apache-2.0. O modelo (`ggml-base.bin`, os pesos do Whisper da OpenAI convertidos pelo whisper.cpp) é MIT e não vai no binário nem no repositório: o núcleo baixa quando você liga o ditado (ou os comandos falados).

### A voz da abelha e o ouvir o nome

Para falar com a voz da abelha ([decisão 0015](decisoes/0015-comandos-falados-e-voz-da-abelha.md)), o `colmeia-voz` ganhou um crate (e o que ele traz). Os fonemas, o efeito, o fim da fala e o reconhecimento do nome (MFCC, FFT e DTW) são código daqui, sem dependência.

| Crate | Versão | Licença | Para quê |
| --- | --- | --- | --- |
| ort, ort-sys | 2.0.0-rc.14 | MIT OR Apache-2.0 | Rodar o Kokoro pelo ONNX Runtime, carregado do arquivo que o núcleo baixou (`load-dynamic`: nada é baixado na compilação) |
| libloading | 0.9 | ISC | Abrir a biblioteca do ONNX Runtime (dependência do ort) |
| smallvec | 1 | MIT OR Apache-2.0 | Dependência do ort |

Baixados pelo núcleo só quando você liga a voz da abelha (Linux x86-64), conferidos pelo tamanho e pelo SHA-256 fixados em `nucleo/internal/voz`; nenhum vai no binário nem no repositório:

| Arquivo | Origem | Licença |
| --- | --- | --- |
| `kokoro-v1.0-q8.onnx` (92 MB) | Kokoro-82M v1.0 (hexgrad), convertido e quantizado pela comunidade ONNX no Hugging Face (`onnx-community/Kokoro-82M-v1.0-ONNX`, revisão `1939ad2a`) | Apache-2.0 |
| `pf_dora.bin` (0,5 MB) | A voz feminina pt-BR do Kokoro, do mesmo repositório | Apache-2.0 |
| `libonnxruntime.so.1.31.0` (29 MB, de um pacote de 11 MB) | ONNX Runtime 1.31.0 da Microsoft, release oficial no GitHub | MIT |

As referências do nome padrão (`voz/recursos/nome-colmeia.json`, só números MFCC, sem áudio) foram feitas com vozes do próprio Kokoro (Apache-2.0) pelo `examples/nome_padrao.rs`. O espeak-ng, que o Kokoro usa para os fonemas em Python, é GPL e não entra: os fonemas do português são feitos por regras em `voz/src/fala/fonemas.rs`.

O crate `canal` passou a usar serde e serde_json (já no binário da tela e da abelha) para a conversa com o `colmeia-voz`.

Para conferir de novo:

```bash
go -C nucleo build -o /tmp/colmeia-nucleo ./cmd/colmeia-nucleo
go version -m /tmp/colmeia-nucleo        # os módulos que entram no binário
go -C nucleo list -m -f '{{.Path}} {{.Dir}}' all   # onde está o LICENSE de cada um
cargo tree -p colmeia-voz -e normal                 # o que entra no colmeia-voz
cargo tree -p colmeia-voz -e normal -f "{p} {l}"    # com a licença de cada um
```
