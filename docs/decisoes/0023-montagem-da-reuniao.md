# 0023 · A reunião montada pelo núcleo: tópico, frase e cartões por tarefa

**Decidido em 13/11/2026.**

## Contexto

A pessoa apresenta a daily e a sprint ao time e, mesmo com a daily em blocos (decisão 0022), voltava ao editor de texto para montar o roteiro: um cabeçalho por cliente ou sistema, itens curtos começando pelo substantivo, o sintoma e a causa quando foi um erro, o que é novo com o nome técnico (a coluna, o endpoint, a tela) e a tecnologia, o fluxo numerado de uma integração e, no fim, como testou e o que falta. O slide dela é uma tabela com um tópico de poucas palavras e uma frase. O agente da tarefa escrevia a nota sem esse jeito e escondia o nome técnico do que criou, e nenhum quadro mostrava o fluxo de uma tarefa com as imagens. Ela pediu uma lousa por daily e por sprint, montada por um agente especializado a cada mudança da tarefa (também no meio dela, porque a daily pode cair no meio), com as tarefas em cima e o fluxo descendo em setas, o último cartão ligado à tarefa seguinte.

## Decisão

- **O estilo é um texto só** (`internal/formato.Estilo`), escrito a partir das anotações dela, e vai aos três que escrevem para a reunião: o agente da tarefa (a nota da daily), a proposta da sprint e o montador. O que é novo entra com o nome técnico e a tecnologia; o que fica de fora é o que ninguém fala em reunião (arquivo, função, commit, branch, caminho, código, SQL).
- **A montagem é por tarefa e tem dois jeitos.** A simples sai da nota, dos anexos e do que aconteceu, sem IA, na hora, e nunca falta. O montador pede ao Claude Code sem terminal e sem ferramentas (como a decisão 0012) um JSON estrito, conferido campo a campo; se não servir, fica a simples. O montador é automático a cada mudança da tarefa, com um tempo quieto de 3 minutos e um por vez, e só quando algo que entra na montagem mudou; a opção do perfil desliga.
- **O desenho é do núcleo, o conteúdo é do agente.** A lousa da reunião (a da daily de hoje e a da sprint fixa, donos novos da lousa) é desenhada por código, sempre igual: colunas na ordem do deck, setas para baixo, a última ligada à tarefa seguinte, o rótulo da seção acima. Pedir "um quadro organizado" ao modelo era o que dava errado; pedir "o texto de quatro cartões" ele faz bem.
- **O que é dela não se mexe.** As peças do montador levam autor próprio e a tarefa; redesenhar apaga só essas. Mover ou editar uma peça montada a torna dela.
- **O slide é a tabela dela.** A apresentação abre em papel (o tema Leitura), o slide do bloco é a tabela tópico | frase com um fio entre as linhas e o slide da tarefa mostra os cartões; Q abre o palco da lousa da reunião. A página da daily e da sprint mostram o tópico e a frase de cada tarefa, com o título da tarefa e a origem embaixo, e o botão direito remonta (sem ou com IA).

## Alternativas

- **O agente da tarefa desenhar a lousa da reunião:** ele está no meio do contexto técnico, escreve no estilo dele e não existe para tarefas sem agente; o desenho sairia diferente a cada vez. Recusado.
- **Só montar ao concluir:** a daily pode cair no meio de uma tarefa, e ela pediu que o quadro acompanhe. Por isso o montador roda a cada mudança, coalescido.
- **Guardar a montagem na corrente de eventos:** é refeita a toda hora e pode ser apagada; fica fora, como as notas (decisão 0006).

## Consequências

- O montador gasta tokens da conta do Claude Code do perfil, no máximo uma vez a cada 3 minutos por tarefa que mudou, e a opção desliga por perfil. Sem o Claude Code instalado, fica a simples.
- A lousa da reunião de um dia passado não é redesenhada: fica como ficou.
- O texto copiado da daily continua saindo das notas (o texto dela); a tabela tópico | frase vive no slide e na página.

16/11: a montagem feita por falta dentro de um pedido não avisa a tela com `montagem.mudou` e é feita uma de cada vez. Na primeira abertura da daily no uso real, cada uma das 21 montagens avisava a tela, a tela pedia a daily e a sprint de novo a cada aviso, e os pedidos que chegavam antes da gravação montavam tudo outra vez: 612 pedidos em fila numa conexão só de banco, 44 s, e a página em erro. A tela também junta uma rajada de avisos num pedido só, meio segundo depois do primeiro.
