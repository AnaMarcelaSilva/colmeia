# 0024 · A reunião como leitor: pauta, painel e a daily que se fecha

**Decidido em 16/11/2026.**

## Contexto

Com a reunião montada (decisão 0023), a daily e a sprint mostravam o tópico e a frase de cada tarefa numa linha, e a parte técnica (a coluna nova, o endpoint, a linguagem, como testou) ficava só nos cartões da lousa da reunião. A pessoa mostrou o Histórico, com a lista das concluídas à esquerda e a tarefa escolhida inteira à direita, e pediu a daily e a sprint nesse modelo, mais técnicas, com as imagens passando com seta já na página, e uma apresentação igual, em que ela entra na tarefa e vê a lista inteira. Pediu também que a ordem da lista fosse dela, que desse para marcar, durante a reunião, o que já foi apresentado, e um "Daily apresentada" que fecha a daily do dia e começa a próxima com o que ficou aberto. No canvas, entre o leitor, a pauta em cima e as linhas que abrem, escolheu o leitor.

## Decisão

- **A pauta é a lista e a ordem é da pessoa.** A daily e a sprint viraram pauta à esquerda (por projeto na daily, por assunto na sprint fixa) e painel à direita. A pauta tem o visto de apresentada, a bolinha do estado, o tópico, o número e o título; a alça arrasta para mudar a ordem, que fica guardada por reunião (`pauta`: perfil, tipo, chave do período, tarefa, ordem, apresentada_em). O núcleo devolve o deck já na ordem dela: dentro da seção, as tarefas com ordem primeiro, as outras depois como vieram.
- **O painel é a tarefa inteira, técnica.** O tópico, o estado, a frase para quem não é dev, os cartões da montagem (o que mudou, o endpoint, a coluna, como funciona, como testou, o que falta) e a galeria com as imagens que a montagem escolheu primeiro e as outras depois, passando com seta e com a legenda. As ações ficam embaixo: apresentada, editar a nota, abrir a tarefa, tirar da reunião.
- **A apresentação é o mesmo leitor em tela cheia.** O "Apresentar" e o F5 abrem a pauta compacta, a tarefa em letra maior e a barra das teclas (↑↓ tarefa, ←→ imagem, espaço apresentada, Enter abre, Q lousa, Esc sai). A apresentação em slides continua no "⋯" e no Shift+F5.
- **"Daily apresentada" fecha a daily do dia.** Guarda a hora (`daily_apresentada`) e zera os vistos. A próxima daily começa dali: a tarefa que concluiu (ou sumiu) sem nada novo desde então já foi falada e sai; o que ficou aberto continua, com a história recente. A ordem fica.
- **A lousa da reunião fica como está.** O leitor serve para ler, preparar e apresentar; a lousa, para o fluxo com setas.

## Alternativas

- **Pauta em cima (chips) com o painel na largura toda:** melhor para projetar, pior para ver a lista inteira e mudar a ordem; a lista virava gaveta. Recusada.
- **As linhas de hoje abrindo no lugar:** mudava menos, mas a tarefa aberta empurrava as outras para fora da vista e a pauta deixava de ser uma pauta. Recusada.
- **Guardar a ordem no slide ou na tarefa:** a ordem é da reunião (a mesma tarefa pode vir em outra posição na sprint). Por isso a chave é o período das notas.

## Consequências

- Três telas novas (daily, sprint e tela cheia) usam um componente só (`registro/leitor.rs`); a apresentação em slides continua existindo, atrás do "⋯".
- Banco na versão 16, só com tabelas novas; nada migra.
- A pauta muda de uma tela para a outra pelos avisos `pauta.mudou` e `daily.apresentada`, juntados numa busca só.
