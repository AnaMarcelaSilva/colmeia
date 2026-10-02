# 0005 · Estado do agente por heurística na saída do terminal

**Decidido em 02/10/2026.**

## Contexto

O que mais importa na Colmeia é saber quando um agente terminou a vez dele, pede aprovação ou deu erro. O Claude Code e o Codex não saem do processo quando terminam uma resposta: voltam ao campo de digitação e param de escrever. O código de saída só diz algo quando o processo acaba de fato.

## Decisão

- O núcleo acompanha cada agente pela saída do terminal, sem varredura periódica: cada leitura só grava o momento (atômico); um único timer por agente olha o silêncio quando dispara, e só é armado de novo se ainda houver o que decidir. Um agente parado não custa nada.
- Depois de 5 s sem saída, o núcleo procura os padrões de aprovação da ferramenta (`detector.go`) no fim do que ela escreveu depois da sua última digitação, já sem as sequências de cor. O que veio antes (um pedido já respondido) e o eco do que você digita (até 500 ms depois de cada tecla) ficam de fora. Se achar, o agente "pede aprovação". Nas ferramentas de IA, o silêncio sozinho já é "sua vez" (o spinner delas escreve sem parar enquanto pensam). No terminal comum, só um BEL fora de uma sequência OSC conta; senão, depois de 60 s, ele fica "parado".
- O eco da digitação e o redesenho depois de um redimensionamento (até 500 ms depois) não contam como trabalho: só olhar o terminal não muda o estado.
- O motivo é sempre um texto de uma lista fixa (`pede aprovação`, `esperando resposta`). O conteúdo do terminal nunca sai do detector: nem para os eventos, nem para o banco, nem para o log.
- Quando a Colmeia fecha o terminal (ao remover o agente ou encerrar o núcleo), o estado fica congelado: o que o programa escreve ao sair não é trabalho e não move o cartão.
- O cartão anda sozinho entre "Agente trabalhando" e "Aguardando você" (`coluna_auto`). Um terminal comum aberto não tira a tarefa do Backlog: parado, ele não é trabalho acontecendo. Se você mover o cartão, o núcleo não desfaz. Nada vai para Revisão ou Concluído sozinho: o fim do processo não quer dizer que a tarefa acabou.
- O fim do processo é classificado pelo código: 0 e 130 (Ctrl+C) terminaram; morte por sinal que não veio da Colmeia é "interrompido"; outro código numa ferramenta de IA é erro. O terminal comum nunca é erro (ele sai com o código do último comando).
- Na tela o estado aparece como leitura, não como certeza: "Parece pedir aprovação".

## Alternativas

- **Ganchos do Claude Code** (`--settings` com Notification e Stop chamando o núcleo): mais confiáveis, mas mexem na configuração do agente e só valem para uma ferramenta. Ficam como melhoria futura, a conferir antes.
- **Ler a tela inteira e interpretar**: mais frágil e mais caro, e levaria o conteúdo do terminal para fora.

## Consequências

- Quando uma ferramenta mudar o texto da aprovação, o motivo cai para "sua vez" até o padrão ser atualizado; o estado (espera você) continua certo.
- Os testes usam bytes fabricados e tempos curtos; o QA usa um `claude` falso. A conferência com o Claude Code de verdade é manual.
