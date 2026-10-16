# 0014 · Ditado por voz num processo à parte

**Decidido em 16/10/2026.**

## Contexto

Falar em vez de digitar na caixa de mensagem da tarefa ajuda quando o pedido ao agente é longo. A transcrição precisa ser local (sem rede e sem tokens), e o Whisper, mesmo no tamanho base, ocupa uns 150 MB de memória e usa vários núcleos enquanto transcreve. A captura de áudio traz bibliotecas de som e o whisper.cpp, que é C++. Quem não usa a voz não pode pagar por nada disso, nem em memória, nem no tamanho do binário, nem em threads.

## Decisão

- **Um programa à parte, `colmeia-voz`,** no mesmo workspace, com a captura (cpal) e o Whisper (whisper-rs, que compila o whisper.cpp junto, só CPU). A tela abre o programa quando você aperta o microfone e fala com ele por uma linha de JSON na entrada e na saída padrão (`ouvir`, `parar`, `cancelar`, `sair`; responde `ouvindo`, `transcrevendo`, `texto`, `erro`, `saindo`). Sem porta, sem socket, sem arquivo. O programa carrega o modelo numa thread enquanto você fala, transcreve em português e, depois de 2 minutos sem pedido, sai e leva o modelo junto. Fechar a entrada (a tela fechou) também faz ele sair.
- **O áudio fica só na memória do `colmeia-voz`.** Até 2 minutos por fala (7,7 MB a 16 kHz); depois disso a gravação para sozinha e transcreve. Silêncio e fala curta demais são recusados antes do Whisper, que inventaria uma frase a partir do nada. A tela recebe só o texto, que entra no rascunho para revisar: nunca é enviado sozinho.
- **O núcleo cuida do modelo e da opção.** O ditado é uma opção do perfil (`ditado`, desligada por padrão) e só liga com o modelo no disco. O núcleo baixa o Whisper base multilíngue (`ggml-base.bin`, 147.951.465 bytes) do repositório oficial do whisper.cpp no Hugging Face, preso a uma revisão, só por https (os redirecionamentos também), confere o tamanho e o SHA-256 fixados no código enquanto baixa e só então dá o nome final ao arquivo, em `<dados>/voz` (pasta 0700, arquivo 0600). O progresso vai às telas do perfil por evento, o download pode ser cancelado (o parcial é apagado) e, pronto, o ditado liga no perfil que pediu. Apagar o modelo desliga o ditado em todos os perfis. O núcleo usa só a biblioteca padrão do Go para isso.
- **No Linux, o microfone vem do PulseAudio** (ou do pipewire-pulse), pelo protocolo nativo. Pelo ALSA, o plugin do pulse acordava a thread de captura sem parar (medido: 70% de um núcleo só para ouvir); pelo PulseAudio, cerca de 1%. Sem servidor de som, cai no ALSA. `PULSE_SOURCE` escolhe a fonte, como nos programas do PulseAudio.
- **Microfone na tela:** o botão ao lado do Enviar abre e fecha (clique e clique); Ctrl+Shift+M é segurar para falar. Ouvindo, o botão fica vermelho e a linha de dica diz "Microfone ligado". Sair da tarefa ou abrir um diálogo com o microfone aberto fecha sem transcrever. Sem animação: a tela parada continua sem redesenhar.

## Alternativas

- **Feature do Cargo no binário da tela:** quem compila sem a feature não teria o ditado, e quem compila com ela carregaria o whisper.cpp e as bibliotecas de som no processo da tela, com o modelo preso à memória dela. Um processo à parte dá o mesmo "custo zero" para quem não usa, sem dois binários diferentes da tela.
- **A transcrição no núcleo:** o núcleo é Go; o Whisper entraria por cgo, que hoje o núcleo não usa (o binário é estático e cruza para o Windows sem compilador C). E o núcleo fica rodando o tempo todo: o modelo ficaria na memória dele.
- **O download no `colmeia-voz`:** traria um cliente HTTPS com TLS para o Rust (rustls, ring), que o projeto não tem. No núcleo, o Go já tem tudo na biblioteca padrão.
- **Modelos quantizados (q5_1, 57 MB):** menores, mas o pedido foi o base multilíngue; trocar o modelo é mudar quatro valores em `nucleo/internal/voz`.

## Consequências

- Um binário a mais no instalador e na release (Linux e Windows). Compilar o `colmeia-voz` pede CMake, um compilador C++, o Clang (libclang, para o bindgen) e, no Linux, `libasound2-dev`. Na release, o whisper.cpp é compilado com `GGML_NATIVE=OFF` (AVX2, FMA e F16C), para rodar em qualquer x86-64 de 2013 para cá; compilado na máquina, usa tudo o que o processador tem.
- Medido num i5-1135G7 (4 núcleos, 8 threads) com uma fala de 7,7 s em pt-BR (TTS do sistema): modelo carregado e parado, 155 MB e nenhuma CPU; ouvindo, 1% de um núcleo; transcrevendo, 2,9 s com 4 threads (10,4 s de CPU) e pico de 335 MB; depois, de volta a 160 MB parado até sair (2 min) e 0 depois. Com o ditado desligado, nada roda.
- A voz do TTS do sistema é robótica e o base erra bastante com ela; com voz de gente o base acerta bem mais, mas continua sendo o menor modelo multilíngue útil. Um modelo maior é outra decisão (e outro tamanho para baixar).
- O microfone de verdade não foi usado nos testes automáticos (nem na medição, que ouviu a saída de som parada); o que dá para testar sem ele tem teste: o protocolo, a conversão de áudio, o ciclo de vida do processo, o download com hash ou tamanho errado, o cancelamento e a opção no perfil.
