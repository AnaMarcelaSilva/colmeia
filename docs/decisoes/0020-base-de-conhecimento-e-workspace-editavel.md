# 0020 · A base de conhecimento e o workspace editável

**Decidido em 19/10/2026.**

## Contexto

Em cada tarefa a usuária repetia ao agente as mesmas coisas do lugar onde ele trabalha: como o nome de um arquivo é gerado, que projeto olhar junto, o padrão dos logs, que as pastas com números são os logs de cada aparelho. Ela pediu uma base de conhecimento por workspace e por projeto, que os próprios agentes pudessem alimentar, para todo agente já começar sabendo. Também pediu para editar o workspace como um projeto, com nome e qualquer imagem como ícone.

## Decisão

- **As notas ficam no núcleo, por workspace ou por projeto.** A tabela `conhecimento` (versão 10 do banco) guarda título (até 80 caracteres) e texto (até 4.000), de onde veio (`voce` ou `agente`) e o estado (`aprovada` ou `sugerida`). A nota é do workspace (vale para todos os projetos dele) ou de um projeto, nunca dos dois (um CHECK garante). Remover o projeto leva as notas dele. Título e texto passam pelo mesmo filtro de senhas das notas da daily.
- **O agente recebe as aprovadas ao começar, num arquivo.** Ao abrir o Claude Code, o núcleo monta o texto (as do workspace primeiro, depois as do projeto, na ordem em que nasceram) e passa por `--append-system-prompt-file`, num arquivo 0600 ao lado do token do agente, apagado junto com ele. A linha de comando qualquer usuário da máquina lê, então o texto não vai em argumento. O texto tem até 8.000 caracteres (perto de 2 mil tokens); o que não couber vai só pelo título, e o agente lê inteiro com `ler_conhecimento`. Sem notas, nada muda na linha de comando e nenhum token é gasto.
- **O que o agente anota é sugestão.** A ferramenta `anotar_conhecimento` (título, texto, `onde` projeto ou workspace e, para corrigir, `substitui` com o id da nota) grava a nota sugerida, que não vai a nenhum agente até você aprovar. Aprovar uma correção passa o texto para a nota e apaga a sugestão. Editar uma sugestão é aprová-la com as suas mudanças. Cada workspace aceita até 20 sugestões esperando; passou disso, o agente ouve que precisa esperar. A opção "Aceitar na hora o que os agentes anotarem", por workspace, nasce desligada.
- **Os eventos não levam o texto.** `conhecimento.criado`, `sugerido`, `editado`, `aprovado` e `removido` levam só os números e o estado: o histórico não se apaga, e uma senha colada por engano sai com a nota. A tela recebe `conhecimento.mudou` com o selo das sugestões do workspace já contado e lê a lista de novo.
- **A tela Conhecimento.** Na barra lateral, abaixo da Lousa do workspace, com o selo âmbar das sugestões; também no menu do botão direito no quadrado do workspace. As sugestões ficam no topo, com Aprovar, Editar antes e Recusar e o que cada uma muda. Depois vêm as notas de todo o workspace e as de cada projeto, com quanto um agente dali recebe. O estado vazio explica com exemplos e oferece a primeira nota.
- **O workspace editável.** O lápis no topo do painel e "Editar workspace…" no menu do quadrado abrem o nome e a imagem. A imagem pode ser PNG, JPEG, GIF ou WebP de até 20 MB e 40 megapixels. O núcleo corta o quadrado do meio, reduz para 128 px e grava só esse PNG no banco (`workspaces.icone`, com um resumo em `icone_versao` que a tela usa para saber quando buscar de novo). A prévia do diálogo vem do mesmo corte (`POST /v1/icone/previa`), então mostra exatamente o que vai ficar. Os projetos levam o nome e a versão da imagem do workspace, e `workspace.mudou` atualiza todas as telas.

## Alternativas

- **Um `CLAUDE.md` na pasta do projeto:** já funciona para quem usa só o Claude Code, mas mexe no repositório da usuária, não vale para o workspace inteiro e não tem aprovação do que o agente escreve.
- **As notas nas instruções do servidor MCP:** são do servidor, iguais para todo agente, e o Claude Code pode cortá-las; o prompt do sistema é o lugar do que muda por projeto.
- **`--append-system-prompt` com o texto:** apareceria na lista de processos para qualquer usuário da máquina.
- **O agente gravar direto, sem aprovação:** um agente que leu um arquivo com instruções escondidas poderia ensiná-las a todos os próximos. A aprovação é o ponto em que uma pessoa lê.
- **Reduzir a imagem na tela:** a tela só lê PNG, e a regra do tamanho ficaria fora do núcleo; com o núcleo cortando, a prévia e o gravado são o mesmo.

## Consequências

- O Claude Code grava o prompt do sistema da conversa na primeira vez e reusa até ela ser compactada: uma nota nova vale para as conversas novas, e as abertas leem com `ler_conhecimento`.
- Cada nota aprovada entra no começo de toda conversa do projeto; o limite de 8.000 caracteres segura o custo.
- O seletor de arquivo (rfd) usa o portal do sistema, que abre na área de trabalho de verdade mesmo com a janela num Xephyr; nos testes, a imagem vai pela API.
