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
| Armazenamento | SQLite em modo WAL | Estado e eventos só acrescentados, encadeados por hash |
| Eventos | WebSocket por perfil | Leva à tela, na hora, cada mudança e o estado dos agentes |
| Extensões (planejado) | servidores MCP | Integrações (GitHub, Docker, banco) e o próprio núcleo para os agentes |

## Protocolo `/v1`

| Rota | Para quê |
| --- | --- |
| `GET /v1/versao` | Versão do protocolo e do núcleo, se o modo demonstração está ligado |
| `GET /v1/estatisticas` | Bytes lidos dos terminais |
| `GET /v1/terminais/{id}` | Só com `--demo`: WebSocket de um terminal de teste |
| `GET /v1/ferramentas` | Ferramentas de agente instaladas e se permitem conta separada |
| `GET` / `POST /v1/perfis` | Listar e criar perfis |
| `PATCH /v1/perfis/{id}` | Mudar o tema do perfil (`tema`) ou o aviso antes de capturar (`aviso_captura`) |
| `GET /v1/perfis/{id}/quadro` | Retrato do perfil numa resposta só: `{seq, projetos, tarefas, agentes}`, com o estado e o último fim de cada agente |
| `GET /v1/perfis/{id}/eventos` | WebSocket de eventos do perfil (só do núcleo para a tela; veja abaixo) |
| `GET /v1/perfis/{id}/linha-do-tempo` | Dias do perfil, do mais novo ao mais antigo; `projeto=`, `de=`/`ate=` (AAAA-MM-DD), `antes=` (paginação) e `limite=` (1 a 500, padrão 200) |
| `GET /v1/perfis/{id}/resumo?tipo=daily` | Daily: `{periodo, ontem, hoje, texto}`; aceita `projeto=` |
| `GET /v1/perfis/{id}/resumo?tipo=sprint` | Sprint de `de` a `ate` (até 92 dias), ou `ultimos=N`, ou `mes=atual`; `formato=markdown` devolve `text/markdown` |
| `POST /v1/tarefas/{id}/anexos` | Anexa um PNG (corpo `image/png`, até 8 MB) à tarefa; `origem=captura\|colagem\|mensagem`, `agente=` e `legenda=` opcionais; devolve `{id, caminho, largura, altura}` |
| `POST /v1/perfis/{id}/anexos` | O mesmo, sem tarefa |
| `GET` / `DELETE /v1/anexos/{id}` | O PNG (imutável, `nosniff`) ou remover (o arquivo sai, o evento fica) |
| `POST /v1/encerrar` | Desliga o núcleo (responde 202 antes); é o que `colmeia-nucleo --encerrar` chama |
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

### Eventos

O WebSocket `GET /v1/perfis/{id}/eventos` usa o mesmo token e recusa `Origin` de outro site. A tela não manda nada por ele: o que chegar é descartado. Cada mensagem é um JSON com `tipo` e `seq`, e as que vêm do histórico trazem também `evento` (o id gravado):

| Tipo | Campos |
| --- | --- |
| `ola` | `seq` atual, `nucleo` (versão) e `aviso` se o histórico foi alterado por fora |
| `recarregar` | A tela ficou para trás e perdeu mensagens: deve pedir o retrato de novo |
| `projeto.criado` / `projeto.removido` | `projeto` (o objeto) / `projeto_id` |
| `tarefa.criada` / `tarefa.atualizada` | `tarefa` (o objeto inteiro) e `origem` (`voce` ou `automatico`) |
| `tarefa.removida` | `tarefa_id`, `projeto_id` |
| `agente.criado` / `agente.removido` | `agente` / `agente_id`, `tarefa_id` |
| `agente.iniciou` | `agente_id`, `tarefa_id`, `desde`, `desde_hora` |
| `agente.estado` | `agente_id`, `tarefa_id`, `estado` (`trabalhando`, `aguardando`, `ocioso`), `motivo` (`pede aprovação` ou `esperando resposta`), `desde`, `desde_hora` |
| `agente.terminou` | `agente_id`, `tarefa_id`, `fim` (`codigo`, `erro`, `motivo`, `hora`, `texto`) |
| `anexo.adicionado` / `anexo.removido` | `anexo_id`, `tarefa_id` |

Aplicar a mesma mensagem duas vezes não muda nada (cria ou atualiza pelo id). Para não perder nada: a tela abre o WebSocket, pede o `/quadro` (que traz o `seq` já incluído nele) e aplica só as mensagens com `seq` maior. Quem publica nunca espera: cada tela tem uma fila de 256 mensagens e, se ela encher, recebe um único `recarregar`. Tipos desconhecidos são ignorados pela tela.

### Terminais

O terminal de um agente nasce no tamanho que a tela informa ao criar ou iniciar o agente (`cols` e `rows` no corpo), e a tela repete o tamanho ao conectar (`?cols=&rows=`), antes do histórico: um programa como o Claude Code desenha logo ao abrir, e um desenho feito em outra largura fica embaralhado. Só o terminal em foco muda o tamanho; as miniaturas mostram a mesma grade com letra menor.

Mensagens de controle da tela para o núcleo no WebSocket (JSON): `{"cols":120,"rows":40}` redimensiona, `{"ack":65536}` confirma o que foi desenhado e `{"intervalo":250}` muda o ritmo de envio em milissegundos. Do núcleo para a tela, `{"fim":true}` avisa que o programa do terminal terminou.

## Agentes

O núcleo é dono dos terminais: fechar a tela não encerra os agentes, e a tela, ao abrir, se liga de novo aos que estão rodando. Cada agente roda a ferramenta (ou o shell do usuário) num pseudo-terminal próprio, na pasta da tarefa, em sessão e grupo de processos próprios. Ao encerrar, o grupo recebe SIGHUP, como ao fechar uma janela de terminal, e só é forçado se não terminar em 3 segundos; assim o Claude Code salva a conversa.

### Estado do agente

O núcleo acompanha a saída de cada terminal sem varredura periódica: cada leitura grava só o momento, e um único timer por agente decide quando dispara.

| Estado | Quando |
| --- | --- |
| `trabalhando` | Saída nova no terminal (que não seja o eco da digitação ou o redesenho até 500 ms depois de um redimensionamento) |
| `aguardando` · `pede aprovação` | 5 s sem saída e o que a ferramenta escreveu depois da sua última digitação termina com um pedido de aprovação conhecido |
| `aguardando` · `esperando resposta` | 5 s sem saída numa ferramenta de IA, ou um BEL fora de OSC num terminal comum |
| `ocioso` | Terminal comum 60 s sem saída |

O fim do processo vira `agente.terminou` com o motivo: `terminou` (código 0 ou 130, ou terminal comum), `interrompido` (sinal que não veio da Colmeia), `erro` (outro código numa ferramenta de IA), `removido` ou `nucleo_encerrado`.

**Coluna automática.** Um agente de IA que começa numa tarefa do Backlog põe a tarefa em "Agente trabalhando" (um terminal comum não). Quando a Colmeia fecha um terminal, o estado do agente fica congelado até o fim ser gravado. Quando um agente passa a esperar você, a tarefa vai de "Agente trabalhando" para "Aguardando você"; quando ele volta a trabalhar (e ninguém mais da tarefa espera), ela volta, desde que a última mudança tenha sido do núcleo (`tarefas.coluna_auto`). Uma mudança sua zera a marca e o núcleo nunca a desfaz. Nada vai para Revisão ou Concluído sozinho.

Um agente do Claude Code sempre tem um id de conversa: o de uma conversa retomada ou um novo, passado com `--session-id`. Ao iniciar de novo, a conversa que já existe é aberta com `--resume`. As conversas ficam onde o Claude Code guarda: `<configuração>/projects/<pasta com tudo que não é letra ou número trocado por "-">/<id>.jsonl`; a Colmeia lê só o fim de cada arquivo para achar o título.

## Dados

SQLite em `~/.local/share/colmeia/colmeia.db` (modo WAL, diretório 0700). Tabelas de estado (perfis, contas, workspaces, projetos, tarefas, agentes, anexos) e uma tabela `eventos` só de acréscimo: cada mudança grava um evento com o hash do anterior, e o núcleo confere a corrente ao iniciar.

```sql
eventos (id, momento, tipo, dados, hash_anterior, hash,
         perfil_id, projeto_id, tarefa_id, agente_id)   -- colunas derivadas, fora do hash
anexos  (id, perfil_id, tarefa_id, sha256, largura, altura, bytes,
         origem, legenda, criado_em, removido)
```

O hash cobre `anterior|momento|tipo|dados`. Os dados de cada evento começam com `"_escopo":{perfil, projeto, tarefa, agente}`, coberto pelo hash; as colunas derivadas repetem esse escopo para a linha do tempo de um perfil não ler o histórico inteiro, e a verificação confere que batem. Os eventos gravados antes delas foram ligados aos perfis uma única vez (`PRAGMA user_version = 2`), seguindo o próprio histórico: é a única exceção ao "só acréscimo", mexe só nas colunas derivadas e o que não dá para ligar fica sem perfil. As mudanças de tarefa guardam a tarefa inteira (com o título) e o nome do projeto: a linha do tempo não depende de a tarefa ainda existir.

Os anexos ficam em `anexos/<perfil>/<sha256>.png` (`0600`); a linha da tabela sobrevive à tarefa, como o evento. A conta separada de uma ferramenta num perfil fica em `perfis/<id>/contas/<ferramenta>/`, apontada pela variável da própria ferramenta (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`). As cópias isoladas ficam em `copias/<tarefa>-<branch>/`. Colunas novas entram por migração ao abrir o banco, sem perder o que já está gravado.

## Regras de desempenho

1. Tudo por evento, nada de varredura periódica.
2. A tela recebe só o que mudou.
3. Terminal só é desenhado quando está visível.
4. Só o terminal em foco é tempo real: cerca de 60 envios por segundo para ele, 4 para as miniaturas e 1 para quem aparece só no cartão.
5. Controle de fluxo: a tela confirma o que desenhou; acima de 1 MB sem confirmação, o núcleo para de ler o terminal.
6. Listas e quadro virtualizados.
7. Carregamento preguiçoso: integração só inicia quando a branch usa; miniaturas só quando aparecem, num cache limitado.
8. Nenhuma thread acorda por tempo: o terminal da tela dorme no `poll(2)` e os eventos numa leitura sem tempo limite; o estado dos agentes usa um timer por agente, só enquanto há o que decidir.

## Fases

1. **Fundação:** núcleo e canal, perfis, projetos, branches por worktree, terminais com agentes, eventos em tempo real, linha do tempo em SQLite, quadro por projeto, daily e sprint (feito no Linux; falta o Windows).
2. **Orquestração:** núcleo como servidor MCP para os agentes, aprovações em três opções e modo autônomo, receitas como skills, comunicação entre agentes com limite contra loops, notificações e mascote.
3. **Integrações:** servidores MCP de GitHub, Docker por branch e banco, tarefa a partir de link, tela de provedores.
4. **Expansão:** servidores de terceiros, "entender projeto", busca e replay, custos, acesso remoto e celular.
