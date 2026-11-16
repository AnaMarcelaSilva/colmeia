# 0022 · Daily em blocos por projeto e um slide por bloco

**Decidido em 28/10/2026.**

## Contexto

Quem apresenta a daily fala um projeto (ou um cliente) de cada vez: um cabeçalho com o nome e, embaixo, itens curtos, às vezes um parágrafo de explicação, às vezes uma lista numerada de passos com uma linha de pendência no fim. A página da Colmeia agrupava por dia (Ontem, Hoje, Esperando minha resposta) e misturava os projetos; a nota de uma tarefa era tratada como "uma frase" e, com itens, virava um parágrafo colado no título; o texto copiado saía dos eventos, não das notas; e a apresentação tinha um divisor vazio por projeto seguido de um slide por tarefa, com a nota pequena no canto e o slide vazio. O resultado é que a pessoa voltava para o editor de texto para montar a daily à mão.

## Decisão

- **A página é uma lista de blocos, um por seção do deck** (o projeto, na ordem da barra lateral; na sprint fixa, o assunto), com o cabeçalho que recolhe, igual ao da sprint. Com um projeto só no recorte, o cabeçalho não aparece. Dentro do bloco, uma tarefa por linha, na ordem do deck. O dia e o estado saem dos grupos e viram selos na linha ("Ontem", "esperando você", "parou com erro"); o alto da página conta as tarefas que esperam resposta.
- **A nota é desenhada em itens** (`NotaMedida`): cada bloco do markdown é um galley, o marcador fica numa coluna só dele e as linhas que quebram alinham com o texto. Uma nota de uma linha continua na mesma linha do título. A mesma peça serve à página, ao slide do bloco e ao slide da tarefa.
- **O texto copiado é montado na tela a partir do deck**, um bloco por projeto com as notas (itens recuados), o estado entre parênteses quando conta e as tarefas que esperam resposta num bloco final. O texto do núcleo (`resumo.texto`, dos eventos) continua existindo para a abelha e a voz.
- **A apresentação tem um slide por bloco**, com as tarefas e as notas em 22 por escala, dividido em partes pela altura da tela sem cortar tarefa (a medição é feita na primeira pintura e refeita quando a janela muda de tamanho). Só a tarefa com anexos ou lousa ganha o slide dela, logo depois do bloco. Qualquer tarefa abre em detalhe de dentro do bloco (clique ou Enter; Esc volta), que é o slide da tarefa com tudo o que ele já fazia (nota, anexos, pedido ao agente). No slide da tarefa a nota vem logo abaixo do título, em 26 por escala, e "O que foi feito" fica no fim com o que sobra.
- **O agente recebe o formato da nota** num lugar só (`internal/formato`): a descrição de `complementar_nota` leva o contrato inteiro com um exemplo, as instruções gerais do MCP apontam para ela e o pedido da daily (quando a tarefa vai para Revisão ou Concluído) repete o essencial. Primeira linha para quem não é técnico, um item por mudança, "Falta" por último, sem nomes de código, até 600 caracteres, imagem só com tela para mostrar, lousa só a pedido.

## Alternativas

- **Manter os grupos por dia e só pôr o projeto em cada linha:** era o que havia; a pessoa precisava remontar os blocos de cabeça.
- **Um slide por tarefa com a nota maior:** continuaria uma tela por tarefa (14 telas para 8 tarefas) e o bloco que ela lê não existiria.
- **Uma skill em arquivo para o agente:** dependeria de o agente escolher lê-la e de cada ferramenta ter o seu mecanismo; as instruções do MCP e o pedido da daily chegam a todos.
- **A fala do bloco (um texto por projeto e por dia, como a sprint tem por assunto):** faz sentido, mas precisa de tabela, rota e evento novos. Fica para outra rodada; o nome do projeto já é o cabeçalho que ela escreve à mão.

## Consequências

- O `Pagina::Divisor` deixou de existir; a sprint também apresenta por bloco (o assunto como título).
- `tarefa_atual()` vale no slide da tarefa e no detalhe; no bloco, o H esconde a tarefa sob o mouse.
- O texto da daily deixou de trazer o tempo dos agentes (vem das notas); "Mostrar tempo dos agentes" continua valendo para os números e para a sprint.
