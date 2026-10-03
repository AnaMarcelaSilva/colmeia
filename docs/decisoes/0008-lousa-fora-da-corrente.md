# 0008 · A lousa fora da corrente de eventos, desenhada pela própria tela

**Decidido em 03/10/2026.**

## Contexto

A usuária quer um quadro livre por workspace e por tarefa para pensar: notas com markdown, trechos de terminal, imagens, vídeos, cartões de tarefa e ligações entre eles, movidos à vontade. Ela quer também que o agente da tarefa monte ali o fluxo do que está fazendo e que a lousa da tarefa apareça no slide da daily.

O histórico da Colmeia é uma corrente de eventos imutável, encadeada por hash. A tela é egui e desenha tudo por quadro.

## Decisão

- **Mexer na lousa não entra na corrente.** Cada arrasto viraria uma linha imutável para sempre, e o texto das notas ficaria preso no hash. Os itens ficam nas tabelas `lousas` e `lousa_elementos`, com uma versão por item. As telas ficam sabendo das mudanças pelo aviso `lousa.mudou`, que leva os itens inteiros: aplicar só a versão maior que a local deixa a aplicação idempotente e descarta o próprio eco, sem id de cliente.
- **Só o agente entra na linha do tempo**, pelo evento `lousa.agente`, com as quantidades por tipo e os ids dos itens, nunca o texto. Na linha do tempo, ele aparece como "Claude Code (dev) acrescentou 6 itens à lousa de “X”". O núcleo lê os itens na hora de avisar as telas.
- **Conflito por versão, como na nota da daily.** Um lote de operações é tudo ou nada; se alguma versão não bate, a resposta é 409 com o estado atual e nada é gravado. A tela aplica o estado do núcleo ("Mudou em outra tela; atualizei"). O texto aberto no editor não é trocado: ao sair da edição, junta-se como na nota da daily.
- **O agente só acrescenta.** As rotas dele (`/v1/agente/lousa*`) tiram a lousa do token, aceitam até 50 itens por chamada (nota, texto, código, ligação e imagem de um anexo da própria tarefa) e posicionam o que vem sem `x`/`y` numa grade à direita do que já existe (56 entre cartões, para o rótulo de uma ligação caber). Uma posição dada que cobre ou encosta (menos de 24) em outro item anda até o primeiro lugar livre à direita ou abaixo. Sem mover nem apagar, não há conflito com a tela.
- **Imagens e vídeos da lousa são anexos do perfil** (`tarefa_id` nulo, `na_lousa = 1`). Assim não se repetem na coluna "Anexos" do slide nem na linha do tempo.
- **A tela desenha a lousa sozinha**, com o próprio parser de markdown e a câmera (tela ↔ quadro). Só o que aparece é desenhado. O layout dos textos fica em cache por item, revisão e zoom, e a letra que fica menor que 6 px na tela vira barra. O zoom anda em níveis fixos.
- **O desfazer fica na tela.** Desfazer uma remoção recria os itens com ids novos, e a pilha troca os antigos pelos novos (os negativos, de quem ainda não chegou ao núcleo, também).

## Alternativas

- **Registrar cada mudança na corrente:** o histórico cresceria com cada arrasto e guardaria para sempre textos que podem ser apagados (decisão 0006).
- **`egui::Scene` para o quadro infinito:** a transformação da camada deixa o texto borrado no zoom e tira o controle sobre o que é desenhado (o recorte do visível, a troca por barras). Recusado.
- **`egui_commonmark`:** desenha com widgets, sem cache por item nem o zoom da lousa, e acompanha as versões do egui com atraso. O subconjunto pedido (títulos, ênfase, listas, caixas, tabelas e blocos de código) cabe num parser pequeno.
- **Id de cliente para descartar o eco:** a versão por item já resolve, sem estado a mais.
- **Agente movendo e apagando:** traria conflito com a usuária no meio da edição. Fica para depois, se fizer falta.

## Consequências

- Mudanças na lousa não aparecem na linha do tempo nem na daily, só as do agente. Para a daily, a lousa aparece no slide da tarefa, só leitura.
- Duas telas editando o mesmo texto ao mesmo tempo não têm junção campo a campo: vale o 409 e a junção simples.
- Os anexos `na_lousa` que nenhum item usa mais não são limpos sozinhos.
- O nome visível é "Lousa" (para a usuária, "Quadro" já é o kanban). As ferramentas do agente são `ler_lousa` e `acrescentar_a_lousa`.
