# 0017 · Voz que espera a frase, entende os nomes e cabe parada

**Decidido em 17/10/2026.**

## Contexto

Testando com o microfone de verdade, a usuária achou três problemas na voz ([0014](0014-ditado-em-processo-a-parte.md), [0015](0015-comandos-falados-e-voz-da-abelha.md)): o comando fechava antes do fim da frase ("Não ouvi o pedido"), o Whisper base escrevia "go meia" e "honeia" no lugar de "Colmeia", e "Ouvir o nome" não atendia a voz dela (as referências do nome padrão são de voz sintética). Também apareceu "O ONNX Runtime não abriu (only one environment is allowed per process)" na voz da abelha.

## Decisão

- **Fim da fala pelo ruído medido.** O limiar deixou de ter um piso fixo alto (0,008 de RMS, acima da fala de um microfone baixo). O ruído é o trecho mais baixo dos últimos 2 s (a fala sempre tem vãos entre as sílabas; o ventilador, não), medido já nos primeiros 300 ms. Para entrar na fala o trecho passa ~10 dB do ruído; para continuar nela, ~5 dB (as sílabas fracas do fim da palavra não contam como pausa). O comando espera 4 s a fala começar, fecha depois de 1,2 s de silêncio (de 0,6 a 3 s, em Ajustes, guardado no perfil como `voz_pausa`) e vai até 30 s. Clicar na abelha com o microfone aberto agora **para e transcreve** o que ouviu (antes, cancelava): o clique de parar sempre manda. O ditado da caixa continua clique e clique, sem fechar sozinho.
- **O Whisper sempre em português e com o vocabulário do perfil.** O núcleo monta (`comandovoz.Vocabulario`) um prompt inicial de até 800 bytes com o nome da abelha, "Colmeia", as palavras dos comandos e os nomes dos projetos, das configurações do Play e das tarefas (as abertas antes das concluídas, sem cortar um nome no meio), e entrega por `GET /v1/perfis/{id}/voz/contexto` junto com o modelo e a pausa. A tela e a abelha mandam isso no pedido `ouvir` (e no `vigiar`, para o comando depois do nome). A busca é em feixe de 5.
- **O Whisper small como opção.** Em Ajustes, "Modelo de transcrição": Base (148 MB, padrão) ou Small (488 MB, `ggml-small.bin` da mesma revisão do repositório oficial, SHA-256 `1be3a9b2…a987b` e 487.601.967 bytes, os do LFS do Hugging Face, conferidos baixando). Cada um tem o seu gerente de download (o mesmo código). O perfil guarda o preferido (`voz_modelo`); vale o preferido baixado, senão o outro, então só o small no disco usa o small. O ditado e os comandos ligam com qualquer um; sem nenhum, desligam.
- **O ONNX Runtime numa variável estática do processo.** O `ort` aceita um ambiente por processo, mesmo depois de soltar o primeiro: soltar o Kokoro parado e falar de novo dava o erro. Agora o ambiente é criado uma vez (`OnceLock`) e só a sessão do modelo sai da memória. Os erros das bibliotecas vão para o log (`<canal>/colmeia-voz.log`, recomeça ao passar de 1 MB); o balão diz em português o que houve e o que fazer.
- **O nome padrão com a voz de quem usa.** "Gravar o nome com a minha voz" vale também para "Colmeia": as 3 ou 4 gravações viram referências que **somam** às sintéticas (o que já reconhecia continua). "Testar o nome" abre o microfone só em Ajustes (até parar ou 1 minuto) e mostra ao vivo uma nota de 0 a 100 do quanto o que se fala parece o nome, com o traço da sensibilidade escolhida. A escala da sensibilidade ficou mais larga (5% do limiar por passo, eram 3%) e o detector também perdeu o piso fixo de energia.
- **Ouvir o nome mais leve.** Abaixo de 2 vezes o ruído e longe de fala (300 ms), o quadro repete o último silêncio sem calcular a FFT, e o alinhamento com as referências roda a cada 50 ms (eram 30). O tempo parado agora é conferido a cada bloco do microfone: ouvindo o nome, a espera nunca chegava ao fim sozinha e o Whisper podia ficar na memória.

## Alternativas

- **Um VAD com modelo (Silero):** mais um ONNX para baixar e carregar só para achar pausas; a energia contra o ruído medido resolve o caso, com testes de sinais sintéticos.
- **Trocar o base pelo small para todos:** três vezes maior e uns 4 s mais lento por comando; com o vocabulário no prompt o base já escreve os nomes.
- **Substituir as referências sintéticas pelas gravações:** perderia o que já funcionava com outras vozes; somar custa só mais alinhamentos quando há fala.

## Consequências

Medido num i5-1135G7 com falas sintéticas (Kokoro) de 2,5 a 3,2 s, 4 linhas:

| | base | small |
|---|---|---|
| carregar | 0,1 a 0,2 s, 155 MB | 0,2 s, 479 MB |
| gulosa, sem vocabulário | 1,5 s · "Comeia", "Com meia para o servidor do projeto APPEDIDOS" | 5,2 a 5,6 s · "Commeia", "Comeia" |
| feixe 5 com vocabulário | 1,7 a 1,8 s · "Colmeia, rodar o projeto Loja Web.", "…ajustar o cache.", "Colmeia para o servidor do projeto api-pedidos." | 6,4 a 6,5 s · "Colmeia, rodar o projeto loja-web.", "Colmeia, parar o servidor do projeto api-pedidos." |
| pico de memória | 425 MB | ~950 MB |

O vocabulário é o que acerta os nomes; o feixe custa 0,1 s no base e 0,5 s no small. O small só vale para quem aceita esperar uns 6 s por comando.

Ouvindo o nome (monitor de um sink nulo do PulseAudio, nunca o microfone real): em silêncio, de 1,0% para 0,75% de um núcleo; com fala sem o nome, de 5,3% para 3,7%; 7,3 MB. Com o Whisper carregado, 161 MB, e de volta a 8,8 MB quando fica parado. O acerto com as vozes sintéticas de fora das referências não mudou: na sensibilidade 5, 36 de 36 frases com o nome e 3 de 60 sem ele (as 3 têm "colmeia" no meio).

A voz de gente pode errar mais que a sintética: por isso a gravação com a própria voz, o teste e a escala mais larga. Testado em Xephyr com áudio sintético: o nome é ouvido com a janela da Colmeia fechada, só a abelha fixa aberta, e o comando é respondido.
