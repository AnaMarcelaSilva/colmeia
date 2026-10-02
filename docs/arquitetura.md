# Arquitetura

## Hierarquia

Tudo se organiza em Perfil › Workspace › Projeto › Branch. A branch é a unidade isolada, onde os agentes trabalham.

| Conceito | O que é | Exemplo |
| --- | --- | --- |
| Perfil | Contexto de vida, com credenciais, contas, provedor de IA e registros próprios | Profissional, Estudo, Pessoal |
| Workspace | Agrupador de projetos que andam juntos | Empresa X, Curso de Rust |
| Projeto | Um repositório git ou uma pasta de trabalho sem git (sem branches) | loja-web, clientes |
| Branch | Filtro do quadro; a tarefa pode ter uma cópia isolada (git worktree) na branch dela | dev, main |
| Tarefa | Unidade de trabalho do quadro, de um projeto; os agentes trabalham na pasta do projeto ou na cópia isolada | Nova tela de pedidos |
| Agente | Uma ferramenta de IA (ou um terminal comum) com papel definido, rodando na pasta da tarefa | líder, dev, revisor, testador |
| Evento | Tudo que acontece: commit, PR, pergunta do agente, teste, print, aprovação | agente abriu PR na dev |

Perfis não se enxergam. Os eventos são a fonte de verdade: quadro, daily, sprint e busca são visões calculadas sobre eles.

## Peças

| Peça | Tecnologia | Papel |
| --- | --- | --- |
| Núcleo | Go | Dono de todo o estado: terminais, eventos, integrações |
| Tela | Rust + egui (wgpu) | Mostra e pede; nenhuma regra de negócio |
| Terminal | alacritty_terminal | Interpreta a saída dos agentes na tela |
| Canal | socket Unix + token | Liga tela e núcleo sem porta de rede |
| Armazenamento (planejado) | SQLite em modo WAL | Eventos só acrescentados, encadeados por hash |
| Extensões (planejado) | servidores MCP | Integrações (GitHub, Docker, banco) e o próprio núcleo para os agentes |

## Protocolo `/v1`

| Rota | Para quê |
| --- | --- |
| `GET /v1/versao` | Versão do protocolo e do núcleo, se o modo demonstração está ligado |
| `GET /v1/estatisticas` | Bytes lidos dos terminais |
| `GET /v1/terminais/{id}` | Só com `--demo`: WebSocket de um terminal de teste |
| `GET /v1/ferramentas` | Ferramentas de agente instaladas e se permitem conta separada |
| `GET` / `POST /v1/perfis` | Listar e criar perfis |
| `PATCH /v1/perfis/{id}` | Mudar o tema do perfil |
| `GET` / `PUT /v1/perfis/{id}/contas` | Contas de IA do perfil (`sistema` ou `separada`) |
| `GET` / `POST /v1/perfis/{id}/workspaces` | Listar e criar workspaces |
| `GET /v1/perfis/{id}/projetos` | Projetos do perfil, com o workspace |
| `POST /v1/workspaces/{id}/projetos` | Adicionar projeto: um repositório entra pela raiz, com a branch atual; outra pasta entra como pasta de trabalho |
| `DELETE /v1/projetos/{id}` | Tirar o projeto da Colmeia: para os agentes e tira as cópias isoladas (a pasta não é tocada) |
| `GET /v1/projetos/{id}/branches` | Branches locais do repositório (vazio numa pasta sem git) |
| `GET` / `POST /v1/projetos/{id}/tarefas` | Listar e criar tarefas; `local: "copia"` cria a cópia isolada (branch nova a partir de `base`, ou existente) |
| `PATCH` / `DELETE /v1/tarefas/{id}` | Mudar título, coluna ou branch; remover (recusa se a cópia tiver mudanças sem commit) |
| `GET /v1/projetos/{id}/agentes` | Agentes de todas as tarefas do projeto, com `ativo` |
| `GET` / `POST /v1/tarefas/{id}/agentes` | Listar agentes da tarefa; criar um (ferramenta, papel e, no Claude Code, a conversa a retomar) e já abrir o terminal |
| `GET /v1/tarefas/{id}/sessoes` | Conversas do Claude Code guardadas para a pasta da tarefa, e se ele está aberto nela fora da Colmeia |
| `POST /v1/agentes/{id}/iniciar` | Abrir de novo o terminal de um agente parado |
| `DELETE /v1/agentes/{id}` | Encerrar e remover o agente |
| `GET /v1/agentes/{id}/terminal` | WebSocket do terminal do agente: binário é digitação e saída; texto é controle |
| `POST /v1/demo/carga?modo=` | Só com `--demo`: cargas de teste nos terminais |

Erros voltam como `{"erro": "mensagem"}` em português, com 400 (pedido inválido), 404 ou 409 (nome repetido); a tela mostra a mensagem como veio.

Mensagens de controle da tela para o núcleo no WebSocket (JSON): `{"cols":120,"rows":40}` redimensiona, `{"ack":65536}` confirma o que foi desenhado e `{"intervalo":250}` muda o ritmo de envio em milissegundos. Do núcleo para a tela, `{"fim":true}` avisa que o programa do terminal terminou.

## Agentes

O núcleo é dono dos terminais: fechar a tela não encerra os agentes, e a tela, ao abrir, se liga de novo aos que estão rodando. Cada agente roda a ferramenta (ou o shell do usuário) num pseudo-terminal próprio, na pasta da tarefa, em sessão e grupo de processos próprios. Ao encerrar, o grupo recebe SIGHUP, como ao fechar uma janela de terminal, e só é forçado se não terminar em 3 segundos; assim o Claude Code salva a conversa.

Um agente do Claude Code sempre tem um id de conversa: o de uma conversa retomada ou um novo, passado com `--session-id`. Ao iniciar de novo, a conversa que já existe é aberta com `--resume`. As conversas ficam onde o Claude Code guarda: `<configuração>/projects/<pasta com tudo que não é letra ou número trocado por "-">/<id>.jsonl`; a Colmeia lê só o fim de cada arquivo para achar o título.

## Dados

SQLite em `~/.local/share/colmeia/colmeia.db` (modo WAL, diretório 0700). Tabelas de estado (perfis, contas, workspaces, projetos, tarefas, agentes) e uma tabela `eventos` só de acréscimo: cada mudança grava um evento com o hash do anterior, e o núcleo confere a corrente ao iniciar. A conta separada de uma ferramenta num perfil fica em `perfis/<id>/contas/<ferramenta>/`, apontada pela variável da própria ferramenta (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`). As cópias isoladas ficam em `copias/<tarefa>-<branch>/`. Colunas novas entram por migração ao abrir o banco, sem perder o que já está gravado.

## Regras de desempenho

1. Tudo por evento, nada de varredura periódica.
2. A tela recebe só o que mudou.
3. Terminal só é desenhado quando está visível.
4. Só o terminal em foco é tempo real: cerca de 60 envios por segundo para ele, 4 para as miniaturas e 1 para quem aparece só no cartão.
5. Controle de fluxo: a tela confirma o que desenhou; acima de 1 MB sem confirmação, o núcleo para de ler o terminal.
6. Listas e quadro virtualizados.
7. Carregamento preguiçoso: integração só inicia quando a branch usa.

## Fases

1. **Fundação:** núcleo e canal, perfis, projetos em abas, branches por worktree, terminais com agentes, linha do tempo em SQLite, quadro por projeto, daily e sprint, Windows e Linux.
2. **Orquestração:** núcleo como servidor MCP para os agentes, aprovações em três opções e modo autônomo, receitas como skills, comunicação entre agentes com limite contra loops, notificações e mascote.
3. **Integrações:** servidores MCP de GitHub, Docker por branch e banco, tarefa a partir de link, tela de provedores.
4. **Expansão:** servidores de terceiros, "entender projeto", busca e replay, custos, acesso remoto e celular.
