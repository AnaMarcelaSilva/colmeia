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
| MCP dos agentes | `colmeia-nucleo mcp` (stdio) | O próprio núcleo como servidor MCP para os agentes do Claude Code que a Colmeia abre, restrito à tarefa de cada um |
| Navegador | Chrome ou Chromium por `--remote-debugging-pipe` | Uma janela por tarefa, ao lado da Colmeia, para abrir e capturar o que o agente criou |
| Bancos de dados | `database/sql` com pgx, go-sql-driver/mysql, go-mssqldb e o SQLite do modernc | Conexões do perfil só como cliente: árvore, console e consultas dos agentes com aprovação |
| Chaveiro | Secret Service pelo D-Bus (go-keyring) | Onde ficam as senhas das conexões; sem ele, só na memória do núcleo |
| Extensões (planejado) | servidores MCP | Integrações (GitHub, Docker) |

## Protocolo `/v1`

| Rota | Para quê |
| --- | --- |
| `GET /v1/versao` | Versão do protocolo e do núcleo, se o modo demonstração está ligado |
| `GET /v1/estatisticas` | Bytes lidos dos terminais |
| `GET /v1/terminais/{id}` | Só com `--demo`: WebSocket de um terminal de teste |
| `GET /v1/ferramentas` | Ferramentas de agente instaladas e se permitem conta separada |
| `GET` / `POST /v1/perfis` | Listar e criar perfis |
| `PATCH /v1/perfis/{id}` | Mudar o tema do perfil (`tema`), o aviso antes de capturar (`aviso_captura`) ou a exibição do tempo dos agentes no registro (`tempo_agentes`, desligada por padrão; gera o evento `perfil.tempo_agentes`, levado às telas só com o booleano) |
| `GET /v1/perfis/{id}/quadro` | Retrato do perfil numa resposta só: `{seq, projetos, tarefas, agentes, pedidos, navegadores, aprovacoes}`, com o estado e o último fim de cada agente e os pedidos de consulta ao banco esperando você |
| `GET /v1/perfis/{id}/eventos` | WebSocket de eventos do perfil (só do núcleo para a tela; veja abaixo) |
| `GET /v1/perfis/{id}/linha-do-tempo` | Dias do perfil, do mais novo ao mais antigo; `projeto=` ou `workspace=` (nunca os dois: 400; de outro perfil: 404), `de=`/`ate=` (AAAA-MM-DD), `antes=` (paginação) e `limite=` (1 a 500, padrão 200) |
| `GET /v1/perfis/{id}/resumo?tipo=daily` | Daily: `{periodo, ontem, hoje, texto, tempo_agentes}`; aceita `projeto=` ou `workspace=`. Com mais de um projeto no recorte, os blocos citam o projeto e o texto sai separado por projeto |
| `GET /v1/perfis/{id}/resumo?tipo=sprint` | Sprint de `de` a `ate` (até 92 dias), ou `ultimos=N`, ou `mes=atual`; `formato=markdown` devolve `text/markdown` |
| `GET /v1/perfis/{id}/apresentacao?tipo=daily\|sprint` | O deck da apresentação: capa (números com os nomes dos grupos: `concluidas`, `revisao`, `aguardando`, `trabalhando`, `erros` de sessão e `novas`; destaques; partes; tarefas novas) e um slide por tarefa (o que foi feito, números, anexos e a nota); aceita os mesmos parâmetros do resumo (`projeto=` ou `workspace=`, `de=`/`ate=`, `ultimos=`, `mes=`). Cada slide leva a seção (`secao`, `secao_id` e, com mais de um workspace, `workspace`), na ordem da barra lateral. Até 60 slides; o resto vira `mais` |
| `POST /v1/tarefas/{id}/anexos` | Anexa uma imagem à tarefa (corpo `image/png` ou `image/jpeg`, até 8 MB; a foto vira PNG, sem EXIF, e é reduzida acima de 3840 px); `origem=captura\|colagem\|mensagem\|arquivo`, `agente=`, `legenda=` e `nome=` opcionais; devolve `{id, caminho, largura, altura}` |
| `POST /v1/tarefas/{id}/videos` | Anexa um vídeo (`video/mp4`, `video/webm`, `video/x-matroska` ou `video/quicktime`, até 512 MB), copiado em fluxo para o disco; os primeiros bytes precisam bater com o tipo; `nome=` opcional |
| `POST /v1/perfis/{id}/anexos` | O mesmo que o das imagens, sem tarefa; `lousa=1` marca o anexo de uma lousa (fora da linha do tempo e dos slides) |
| `POST /v1/perfis/{id}/videos` | O mesmo que o dos vídeos, sem tarefa; `lousa=1` como acima |
| `POST /v1/workspaces/{id}/lousa` · `POST /v1/tarefas/{id}/lousa` | Abre a lousa do workspace ou da tarefa, criando-a se ainda não existe (sem evento): `{lousa: {id, dono}, elementos, seq}`, com o `seq` lido antes da consulta |
| `GET /v1/lousas/{id}` | A mesma resposta, para recarregar |
| `POST /v1/lousas/{id}/operacoes` | Um lote de até 500 operações, tudo ou nada (corpo até 1 MB, estrito): `criar` (`ref` opcional; `de`/`para` aceitam um id ou o `ref` de um item criado antes no lote), `alterar` (`id`, `versao` e só os campos que mudam; `tipo` só entre nota, texto e código) e `remover` (`id`, `versao`). Responde `{elementos, removidos, refs}` (os removidos incluem as ligações levadas junto); 409 `{erro, elementos, removidos}` se alguma versão não bate, sem gravar nada |
| `GET` / `DELETE /v1/anexos/{id}` | A imagem (PNG imutável, `nosniff`; um vídeo responde 400) ou remover (o arquivo sai, o evento fica) |
| `GET /v1/anexos/{id}/info` | `{tipo, formato, nome, bytes, largura, altura, caminho}`: a tela abre um vídeo no reprodutor do sistema pelo caminho, sempre montado pelo núcleo |
| `PUT /v1/tarefas/{id}/notas` | Nota da tarefa numa daily (`{"tipo":"daily","periodo":"AAAA-MM-DD"}`) ou numa sprint (`"periodo":"AAAA-MM-DD..AAAA-MM-DD"`), até 4.000 caracteres; texto vazio apaga; a que parece ter senha ou chave é recusada (400). Com `versao` (o `atualizada_em` lido, `""` se não havia nota), responde 409 `{erro, texto, versao}` se a nota mudou desde então |
| `GET /v1/tarefas/{id}/pedidos/destino` | Para quem um pedido ao agente iria agora: `{acao: "ativo"\|"reiniciar"\|"novo"\|"bloqueado", agente?, conversa?, outros?, motivo?}` |
| `GET` / `POST /v1/tarefas/{id}/pedidos` | Pedidos da tarefa (`tipo=` e `periodo=` filtram); criar um (`{texto, tipo, periodo, cols?, rows?}`, até 2.000 caracteres, sem controle nem segredo): o núcleo escolhe o agente, inicia ou cria o Claude Code e põe na fila; 409 se o Claude Code está aberto na pasta fora da Colmeia |
| `DELETE /v1/pedidos/{id}` | Cancela um pedido que ainda está na fila |
| `GET /v1/perfis/{id}/pedidos` | Pedidos abertos de todas as tarefas do perfil |
| `GET /v1/navegador` | `{instalado, nome}`: há Chrome ou Chromium para a Colmeia controlar |
| `GET` / `POST` / `DELETE /v1/tarefas/{id}/navegador` | A janela da tarefa: `{aberto, descricao}`; abrir ou trazer para frente (`{url?, x, y, largura, altura}`, geometria em pixels da tela, 0 < lado ≤ 8192; se o endereço não abre, uma janela nova é fechada e a que já existia fica, sem vir para frente); fechar |
| `POST /v1/tarefas/{id}/navegador/captura` | Captura a janela da tarefa e anexa (`origem=captura`, legenda "Navegador: endereço"); devolve `{id}` |
| `PUT /v1/perfis/{id}/apresentando` | `{ativo, geometria?}`: a tela passou a mostrar (ou deixou de mostrar) algo que pode estar compartilhado, a apresentação, a Daily ou a Sprint; o navegador que o agente abre fica fora da tela enquanto isso. `geometria` (`{x, y, largura, altura}`) é onde a janela abre ao lado da Colmeia; menor que 360x300 é ignorada |
| `GET /v1/tarefas/{id}/arquivos?caminho=` | Um nível da pasta da tarefa (só leitura): `{pasta, entradas: [{nome, pasta, link, ignorada, bytes, alterado, sensivel}], mais}`, pastas primeiro, até 2.000 |
| `GET /v1/tarefas/{id}/arquivo?caminho=&mostrar=` | Pré-visualização: texto (até 256 KB, `cortado`), imagem (dimensões e formato), `binario` ou `sensivel` (o conteúdo só vem com `mostrar=1`); sempre com `caminho_absoluto` |
| `GET /v1/tarefas/{id}/arquivo/imagem?caminho=` | A imagem (PNG, JPEG ou GIF) como PNG reduzido a 2048 px |
| `GET` / `POST` / `DELETE /v1/agentes/{id}/mensagens` | Histórico do que você mandou ao agente (da mais nova à mais antiga, até 200), guardar uma mensagem (`{"texto"}`, responde `{"guardada": bool}`: não guarda o que parece senha ou chave) e apagar tudo |
| `GET` / `POST /v1/perfis/{id}/conexoes` | Conexões de banco do perfil (`{conexoes, chaveiro_disponivel}`; cada uma com `senha: "chaveiro"\|"memoria"\|"nenhuma"` e `senha_disponivel`, nunca a senha) e criar uma (`{pasta, nome, tipo, host, porta, usuario, banco, arquivo, ssl, ssl_ca, escrita, agentes, senha?, guardar}`) |
| `GET` / `PATCH` / `DELETE /v1/conexoes/{id}` | Ler; editar (todos os campos, como no criar; a senha vazia ou ausente fica como estava; o tipo não muda) e fechar os pools; remover (a senha sai do chaveiro, o histórico de consultas vai junto, a linha do tempo fica) |
| `PUT` / `DELETE /v1/conexoes/{id}/senha` | `{senha, guardar}`: guarda no chaveiro (se pedido e se houver) ou só na memória e responde `{senha: "chaveiro"\|"memoria"\|"nenhuma"}`. Sem chaveiro agora, uma conexão do chaveiro continua `chaveiro` (a senha fica na memória só nesta sessão). Apagar tira do chaveiro e da memória (a próxima conexão pergunta) |
| `POST /v1/perfis/{id}/conexoes/testar` · `POST /v1/conexoes/{id}/testar` | Testa um rascunho (senha no corpo; vazia com `conexao_id`, vale a salva) ou a conexão salva, numa conexão avulsa: `{ok, ms, servidor, tls}` ou `{ok: false, erro, detalhe}` (a frase simples e a mensagem do driver sem a senha) |
| `POST /v1/conexoes/{id}/desconectar` | Fecha os pools e as execuções da conexão |
| `GET /v1/conexoes/{id}/arvore?nivel=bancos\|esquemas\|objetos\|colunas&banco=&esquema=&objeto=` | Um nível da árvore: `{nomes, padrao}`, `{nomes}`, `{tabelas, views, total, cortado}` (até 20.000 nomes) ou `{colunas: [{nome, tipo, pk, fk}]}` |
| `POST /v1/conexoes/{id}/previa` | `{banco, esquema, objeto}`: confere no catálogo que o objeto existe, cita o nome e roda o `SELECT` das 100 primeiras linhas em leitura: `{resultado, sql}` |
| `POST /v1/conexoes/{id}/execucoes` | `{ficha (16 a 64 letras, algarismos, - ou _), sql, banco?, limite (1 a 5000, padrão 500), tempo_s (1 a 600, padrão 30), confirmar?}`; espera terminar e responde `{colunas: [{nome, tipo, numero}], linhas, mais, ms, afetadas?, verbo, altera}` (cada célula é texto ou null, cortada em 4 KB; binário, inclusive bytes com caracteres de controle numa coluna que não é de texto, vira `<binário 12 KB>`). Uma alteração numa conexão só leitura: 403 `{somente_leitura}`; com escrita ligada e sem `confirmar`: 409 `{precisa_confirmar, verbo, sql, banco, sem_where, confirmacao}` (o nonce vale uma vez, por 2 min, preso a conexão, banco e SQL). Sem a senha: 428 `{precisa_senha}`; tempo esgotado ou cancelada: 422 `{tempo_esgotado}` / `{cancelada}`; erro do banco: 422 `{erro, linha?, do_servidor}`; usuário ou senha recusados pelo servidor: 422 `{erro, detalhe, senha_recusada}` (o 1044 do MySQL, senha aceita sem acesso ao banco, não leva `senha_recusada`). No histórico, a cancelada fica com `resultado: "cancelada"` |
| `GET /v1/execucoes/{ficha}/mais?limite=` · `DELETE /v1/execucoes/{ficha}` | A próxima página da execução aberta (410 se ela já fechou: fica aberta até 2 min parada, uma por conexão); cancelar a que roda ou fechar a aberta |
| `GET` / `DELETE /v1/conexoes/{id}/historico` | As últimas 200 consultas da conexão (com `hora` local) / apagar |
| `GET /v1/perfis/{id}/aprovacoes` · `POST /v1/aprovacoes/{id}` | Os pedidos de consulta dos agentes esperando você; aprovar ou recusar (`{aprovar, motivo?, senha?, guardar?}`: o motivo, até 500 caracteres, vai só para o agente; a senha, quando falta, só para o chaveiro ou a memória). Uma senha recusada pelo servidor responde 422 `{erro, senha_recusada: true}` e o pedido continua pendente, com o mesmo prazo |
| `POST /v1/encerrar` | Desliga o núcleo (responde 202 antes); é o que `colmeia-nucleo --encerrar` chama |
| `GET` / `PUT /v1/perfis/{id}/contas` | Contas de IA do perfil (`sistema` ou `separada`) |
| `GET` / `POST /v1/perfis/{id}/workspaces` | Listar e criar workspaces |
| `GET /v1/perfis/{id}/projetos` | Projetos do perfil, com o workspace |
| `PATCH /v1/workspaces/{id}` | Recolher ou abrir o workspace na barra lateral (`recolhido`); a lista de projetos traz `workspace_recolhido` e as telas do perfil recebem o evento `workspace.recolhido` |
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

Rotas dos agentes (`/v1/agente/*`): só aceitam o token de um agente, e o token da tela recebe 401 nelas (o token de um agente recebe 401 em todas as outras). Nenhuma leva id na URL: a tarefa sai do token.

| Rota | Para quê |
| --- | --- |
| `GET /v1/agente/tarefa` | A tarefa (título, projeto, branch, pasta, coluna), a nota da daily de hoje, a última de sprint, os pedidos abertos e o navegador |
| `GET` / `PUT /v1/agente/nota` | Ler (`tipo=`, `periodo=`) e gravar (`{texto, tipo?, periodo?, modo: "complementar"\|"substituir", pedido?}`); sem tipo e período, vale o do pedido aberto mais recente ou a daily de hoje. Complementar acrescenta depois de uma linha em branco; acima de 4.000 caracteres: 400 "resuma". Com `pedido`, a nota responde o pedido, mas ele só fecha com `concluir` (ou, se o agente esquecer, quando ele terminar a vez e voltar a esperar você) |
| `POST /v1/agente/anexos` | `{caminho, legenda?}`: PNG ou JPEG de dentro da pasta da tarefa, aberto por `os.Root` |
| `POST /v1/agente/navegador` | `{url}`: http, https ou `file://` de dentro da pasta (sem os arquivos sensíveis); abre sem tomar o foco |
| `POST /v1/agente/navegador/captura` | `{anexar?, legenda?}`: o PNG reduzido a 1568 px (base64) e o id do anexo |
| `POST /v1/agente/pedidos/{id}/concluir` | `{resumo?}`: só um pedido da tarefa do token; o resumo, se houver, complementa a nota |
| `GET /v1/agente/lousa` | A lousa da tarefa do token: cada item com id, tipo, posição, tamanho, cor, título, texto, pontas e autor |
| `GET /v1/agente/bancos` | As conexões do perfil com "Agentes podem pedir consultas": `{conexoes: [{id, nome, tipo, banco}]}`, sem host nem usuário |
| `POST /v1/agente/bancos/{id}/consultas` | `{sql, banco?, limite? (até 200)}`: só leitura (uma alteração é recusada, mesmo com escrita ligada na conexão) e sem as funções que leem arquivos do servidor ou derrubam sessões. Cria o pedido, segura o agente em `aguardando · aprovar consulta` e espera a sua resposta por até 5 min (sem resposta, conta como recusa). Aprovada: `{texto, linhas, mais}`, com até 64 KB em colunas separadas por `\|`; recusada, expirada ou cancelada: 409 `{erro, resultado}` |
| `POST /v1/agente/lousa/elementos` | `{elementos: [...]}`: até 50 itens (nota, texto, código, ligação e imagem de um anexo da própria tarefa), com `ref` local para as ligações; sem `x`/`y`, o núcleo põe numa grade à direita do que existe, e sem tamanho estima pelo texto. Só acrescenta; grava `lousa.agente` |

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
| `agente.estado` | `agente_id`, `tarefa_id`, `estado` (`trabalhando`, `aguardando`, `ocioso`), `motivo` (`pede aprovação`, `esperando resposta` ou `aprovar consulta`), `desde`, `desde_hora` |
| `agente.terminou` | `agente_id`, `tarefa_id`, `fim` (`codigo`, `erro`, `motivo`, `hora`, `texto`) |
| `anexo.adicionado` / `anexo.removido` | `anexo_id`, `tarefa_id` (e `agente_id`, se veio de um agente) |
| `nota.atualizada` | `tarefa_id`, `nota_tipo` (`daily` ou `sprint`), `periodo` (o texto da nota não vai no evento); `agente_id` e `modo` quando foi um agente |
| `pedido.criado` / `pedido.entregue` / `pedido.respondido` / `pedido.cancelado` / `pedido.falhou` | `pedido_id`, `tarefa_id`, `agente_id` e `pedido` (o objeto, lido na hora; no histórico só fica o tamanho do texto) |
| `navegador.aberto` / `navegador.fechado` / `navegador.captura` / `navegador.recusado` | `tarefa_id`, `agente_id` (0 é você), `descricao` (esquema, host e caminho, nunca a query) |
| `conexao.mudou` | `acao` (`criada`, `alterada`, `removida`) e `conexao_id`: a tela relê a lista de conexões |
| `banco.consulta` | `conexao_id`, `tarefa_id`, `agente_id`: houve consulta ou alteração (sem SQL nem resultado); a tela relê o histórico se estiver nele |
| `banco.aprovacao` | Fora da corrente (leva o SQL, que só vai para a tela): `acao: "pedida"` com `aprovacao` (`id, conexao_id, conexao, tipo, agente_id, tarefa_id, agente, tarefa, sql, banco, limite, criada_hora, expira, expira_hora, precisa_senha`) ou `acao: "resolvida"` com `aprovacao_id`, `resultado` (`aprovada`, `recusada`, `expirou`, `cancelada`), `linhas`, `ms`, `hora` e `erro` |
| `workspace.recolhido` | `workspace_id`, `recolhido`: o workspace foi recolhido ou aberto na barra lateral |
| `perfil.tempo_agentes` | `perfil_id`, `mostrar`: a opção "Mostrar tempo dos agentes" mudou; as telas do perfil escondem na hora ou buscam de novo ([decisão 0010](decisoes/0010-tempo-dos-agentes-opcional.md)) |
| `lousa.mudou` | `lousa_id`, `dono` (`workspace_id` ou `tarefa_id`), `elementos` (os itens inteiros, com o texto) e `removidos`; com `agente_id` e `evento` quando foi o agente (o evento `lousa.agente` no histórico não tem texto). Mudanças da tela não ficam no histórico ([decisão 0008](decisoes/0008-lousa-fora-da-corrente.md)) |

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
| `aguardando` · `aprovar consulta` | Um pedido de consulta ao banco do agente espera a sua aprovação. O núcleo segura o estado (`Segurar`/`Soltar`): a saída do spinner e o silêncio não mudam nada até você responder |

O fim do processo vira `agente.terminou` com o motivo: `terminou` (código 0 ou 130, ou terminal comum), `interrompido` (sinal que não veio da Colmeia), `erro` (outro código numa ferramenta de IA), `removido` ou `nucleo_encerrado`.

**Coluna automática.** Um agente de IA que começa numa tarefa do Backlog põe a tarefa em "Agente trabalhando" (um terminal comum não). Quando a Colmeia fecha um terminal, o estado do agente fica congelado até o fim ser gravado. Quando um agente passa a esperar você, a tarefa vai de "Agente trabalhando" para "Aguardando você"; quando ele volta a trabalhar (e ninguém mais da tarefa espera), ela volta, desde que a última mudança tenha sido do núcleo (`tarefas.coluna_auto`). Uma mudança sua zera a marca e o núcleo nunca a desfaz. Nada vai para Revisão ou Concluído sozinho.

### Pedidos ao agente

Pela daily, pela sprint ou pela apresentação, você pede algo a mais ao agente de uma tarefa. O núcleo escolhe quem recebe: um Claude Code ativo da tarefa (de preferência um que espera você), senão um parado (iniciado de novo, retomando a conversa), senão um novo, retomando a última conversa da pasta. O pedido fica na fila e entra no terminal por evento, quando o agente passa a `aguardando · esperando resposta` (ou já está assim na criação), nunca durante um `pede aprovação` e nunca com digitação sua nos últimos 5 s. Entra como colagem, com o número do pedido e a instrução de responder pelas ferramentas da Colmeia em até 5 linhas. Um pedido entregue que fica sem resposta continua `entregue` (a tela mostra "parou sem responder"); vira `falhou` só quando o agente termina ou é removido.

### Ferramentas da Colmeia (MCP)

Cada agente do Claude Code que a Colmeia abre ganha um token próprio (32 bytes aleatórios, guardado só em memória pelo sha256) e um `--mcp-config` com o servidor `colmeia`: o próprio `colmeia-nucleo mcp --socket … --token-arquivo …`, por stdio. O token fica num arquivo `0600` em `<canal>/agentes/` (tmpfs), nunca em argumento ou variável de ambiente, e é revogado quando o agente termina, é removido ou o núcleo encerra. A Colmeia passa também `--allowedTools mcp__colmeia`: tudo nelas já está confinado à tarefa. Os outros servidores MCP do usuário continuam valendo (sem `--strict-mcp-config`). Agentes que já rodavam antes desta versão ganham o MCP quando forem iniciados de novo.

Ferramentas: `ler_tarefa`, `ler_nota`, `complementar_nota`, `escrever_nota`, `anexar_imagem`, `abrir_navegador`, `capturar_navegador`, `concluir_pedido`, `ler_lousa`, `acrescentar_a_lousa`, `listar_bancos` e `consultar_banco`. Os argumentos são estritos (campo desconhecido é erro) e cada linha tem até 1 MB. Cada chamada roda na sua goroutine, com o próprio prazo (60 s; 6 min em `consultar_banco`, que espera a sua aprovação), e `notifications/cancelled` cancela a chamada (o pedido de consulta some da tela). A configuração MCP passa `"timeout": 420000` ao Claude Code para ele esperar a aprovação.

### Lousa

O quadro livre de cada workspace e de cada tarefa. A tela desenha só o que aparece, com o layout dos textos em cache por item e zoom; abaixo de 6 px na tela, o texto vira barra. O zoom anda em níveis fixos (10% a 400%). As mudanças vão ao núcleo em lotes, um por vez: mover, redimensionar e cor ao soltar o mouse; o texto 800 ms depois da última tecla, com um único pedido de redesenho. Trocar de tela grava na hora e fechar a janela grava esperando a resposta. Itens criados têm um id negativo até o núcleo responder. O desfazer fica na tela, um comando por gesto (um arrasto, uma sessão de edição). A lousa da tarefa cobre o corpo do terminal em foco, sem mudar o tamanho dele; enquanto isso, o terminal não é desenhado nem recebe o teclado e cai para o ritmo de fundo. No slide da daily, a lousa da tarefa aparece só leitura, ajustada para caber, e o clique abre o palco (tela cheia, de cartão em cartão, na ordem de leitura).

### Navegador da tarefa

Um Chrome ou Chromium por perfil (`COLMEIA_NAVEGADOR`, ou o primeiro de `chromium`, `chromium-browser`, `google-chrome-stable`, `google-chrome` no PATH), com a pasta de perfil `<dados>/navegador/<perfil>/` (0700), nunca a do usuário, e controle só por `--remote-debugging-pipe` (descritores 3 e 4, mensagens separadas por `\0`): nenhuma porta. O balão "Não é possível atualizar o Chrome" fica escondido nesse perfil (`--simulate-outdated-no-au` com uma data distante), porque cobria a página ao lado do agente. Cada tarefa tem a sua janela, posicionada ao lado da Colmeia (ou na metade direita do monitor); no Wayland o compositor pode ignorar a posição. Endereços aceitos: http, https e `file://` de dentro da pasta da tarefa, menos os arquivos sensíveis (os mesmos que a gaveta esconde: `.env*`, `*.pem`, `*.key`, `id_rsa*`…) e o `.git`. A conferência não vale só para o endereço aberto: todo `file://` que a página pede depois (redirecionamento, iframe, imagem, script) passa pela mesma regra pelo domínio `Fetch` do protocolo, e a captura confere o endereço atual da página antes de tirar o print; se a página saiu da pasta, a captura é recusada e vira `navegador.recusado` na linha do tempo. Enquanto a tela mostra algo que pode estar compartilhado (a apresentação, a Daily ou a Sprint), a janela que o agente abre fica fora da área visível (a captura continua funcionando) e vem para o lado quando você clica em "Navegador". Uma página aberta pode tentar instruir o agente (o mesmo risco do WebFetch); as ferramentas da Colmeia só agem dentro da tarefa.

Um agente do Claude Code sempre tem um id de conversa: o de uma conversa retomada ou um novo, passado com `--session-id`. Ao iniciar de novo, a conversa que já existe é aberta com `--resume`. As conversas ficam onde o Claude Code guarda: `<configuração>/projects/<pasta com tudo que não é letra ou número trocado por "-">/<id>.jsonl`; a Colmeia lê só o fim de cada arquivo para achar o título.

## Bancos de dados

O núcleo conecta como cliente aos bancos das conexões do perfil, por `database/sql`: pgx (PostgreSQL), go-sql-driver/mysql (MySQL e MariaDB), go-mssqldb (SQL Server, experimental: só testado por unidade) e o SQLite do modernc, já usado pela Colmeia. Nada escuta porta.

- **Configuração explícita.** O Postgres não deixa o ambiente do usuário entrar: o texto de conexão aceita só as chaves que a Colmeia monta, e depois de lido host, porta, banco, usuário, senha, TLS e parâmetros de sessão são fixados à mão (as `PG*`, o `~/.pgpass` e os certificados de cliente do ambiente não valem). O MySQL é montado por `mysql.Config`, sem DSN em texto, com `LOAD DATA LOCAL INFILE` desligado e sem várias instruções por chamada.
- **TLS:** Desligado, Preferir (tenta com TLS e cai para texto puro; o "Testar" diz qual usou), Exigir (cifra sem conferir o certificado) e Verificar (confere com a CA indicada ou as do sistema e o nome do servidor).
- **Pools** por (conexão, banco), com até 3 conexões, no máximo 4 bancos abertos por conexão; cada um fecha sozinho depois de 10 min sem uso (um timer rearmado a cada uso, sem varredura) e ao editar, remover, desconectar ou trocar a senha.
- **Uma instrução por vez.** O leitor léxico de cada dialeto (aspas, crase, colchetes, `$tag$`, comentários `--`, `#` e `/* */`, inclusive o `/*!…*/` executável do MySQL) divide o texto, acha a instrução sob o cursor e classifica: lê quem começa por `SELECT`, `WITH`, `SHOW`, `DESCRIBE`, `EXPLAIN` (sem `ANALYZE`), `VALUES`, `TABLE` ou um `PRAGMA` só de leitura e não tem, fora de texto, nenhuma palavra que altera (`INSERT`, `UPDATE`, `DELETE`, `INTO`, `SET`, `FOR UPDATE/SHARE`, `CALL`, `EXEC`…); o resto altera. Na dúvida, altera: o erro só faz aparecer uma confirmação a mais. A tela tem o mesmo leitor só para escolher o trecho e colorir.
- **Leitura** roda numa transação só de leitura sempre desfeita no fim: `BEGIN READ ONLY` com `statement_timeout` no Postgres (e `default_transaction_read_only` na sessão de uma conexão sem escrita), `START TRANSACTION READ ONLY` com `max_execution_time` no MySQL, `BEGIN TRAN … ROLLBACK` no SQL Server e o arquivo aberto com `mode=ro` e `query_only` no SQLite. **Alteração** exige escrita ligada na conexão e a sua confirmação, e roda como instrução avulsa (assim `CREATE DATABASE` e `VACUUM` também rodam).
- **Cancelar** cancela o contexto: o pgx manda o CancelRequest, o go-mssqldb o pacote de atenção e o SQLite interrompe; no MySQL, o núcleo roda `KILL QUERY` por outra conexão do pool, senão o servidor continuaria executando. O tempo-limite vale para cada página.
- **Resultado** em páginas de até 5.000 linhas e 8 MB, cada célula como texto (até 4 KB) ou null; uma linha lida a mais diz se há mais sem errar no fim exato. A execução fica aberta para "carregar mais" até 2 min parada, uma por conexão.
- **Erros** passam por um redator que troca a senha (se aparecer) por `***`; a tela recebe a frase simples ("Usuário ou senha recusados.", "Não achou o servidor host:porta.") e o detalhe do driver, e a posição do erro de SQL quando o servidor dá.

**Senhas.** Ficam no chaveiro do sistema (Secret Service pelo D-Bus, serviço `Colmeia` ou `COLMEIA_CHAVEIRO_SERVICO`, conta `conexao:<uuid aleatório>`), com prazo de 30 s por chamada, porque o desbloqueio pode abrir uma janela e esperar. Sem chaveiro (ou com `COLMEIA_CHAVEIRO=memoria`), só na memória do núcleo, até ele encerrar. A coluna `senha` da conexão diz só onde ela está; remover a conexão apaga a entrada do chaveiro.

**Consultas dos agentes.** O pedido fica em memória no núcleo (não na corrente, porque leva o SQL), aparece na tela pelo aviso `banco.aprovacao` (e no `/quadro`, para a tela que reconecta) e segura o agente em `aguardando · aprovar consulta`, que move o cartão para "Aguardando você". Aprovado, roda em leitura (30 s, até 200 linhas, resposta até 64 KB) mesmo que a conexão tenha escrita ligada. Sem aprovação automática nem escrita pelo agente. Veja a [decisão 0009](decisoes/0009-bancos-chaveiro-e-somente-leitura.md).

## Dados

SQLite em `~/.local/share/colmeia/colmeia.db` (modo WAL, diretório 0700). Tabelas de estado (perfis, contas, workspaces, projetos, tarefas, agentes, anexos) e uma tabela `eventos` só de acréscimo: cada mudança grava um evento com o hash do anterior, e o núcleo confere a corrente ao iniciar.

```sql
eventos (id, momento, tipo, dados, hash_anterior, hash,
         perfil_id, projeto_id, tarefa_id, agente_id)   -- colunas derivadas, fora do hash
anexos  (id, perfil_id, tarefa_id, sha256, largura, altura, bytes,
         origem, legenda, criado_em, removido,
         tipo, formato, nome, na_lousa)                -- imagem ou vídeo; png, mp4, webm, mkv, mov
mensagens (id, agente_id, texto, enviada_em)           -- histórico da caixa de mensagem, até 200 por agente
notas   (tarefa_id, tipo, periodo, texto, atualizada_em)   -- daily ou sprint, por período
pedidos (id, tarefa_id, agente_id, tipo, periodo, texto,
         estado, motivo, criado_em, entregue_em, respondido_em)  -- fila, entregue, respondido, cancelado, falhou
lousas  (id, perfil_id, workspace_id, tarefa_id, criada_em)    -- um dono só (CHECK); some com a tarefa
lousa_elementos (id, lousa_id, tipo, x, y, largura, altura, z, cor, titulo, texto,
         anexo_id, tarefa_ref, de_id, para_id, autor, agente_id, versao, atualizado_em)
         -- nota, texto, codigo, imagem, video, tarefa, ligacao; em unidades do quadro
conexoes_banco (id, perfil_id, pasta, nome, tipo, host, porta, usuario, banco, arquivo,
         ssl, ssl_ca, escrita, agentes, chave_segredo, senha, criada_em, atualizada_em)
         -- senha diz onde ela está: chaveiro, memoria ou nenhuma (nunca a senha)
consultas_banco (id, conexao_id, sql, banco, origem, agente_id, momento,
         duracao_ms, linhas, erro, altera, resultado)   -- até 500 por conexão, apagável
```

O hash cobre `anterior|momento|tipo|dados`. Os dados de cada evento começam com `"_escopo":{perfil, projeto, tarefa, agente}`, coberto pelo hash; as colunas derivadas repetem esse escopo para a linha do tempo de um perfil não ler o histórico inteiro, e a verificação confere que batem. Os eventos gravados antes delas foram ligados aos perfis uma única vez (`PRAGMA user_version = 2`), seguindo o próprio histórico: é a única exceção ao "só acréscimo", mexe só nas colunas derivadas e o que não dá para ligar fica sem perfil. As mudanças de tarefa guardam a tarefa inteira (com o título) e o nome do projeto: a linha do tempo não depende de a tarefa ainda existir.

Os anexos ficam em `anexos/<perfil>/<sha256>.<formato>` (`0600`, pasta `0700`); a linha da tabela sobrevive à tarefa, como o evento. A versão 3 do banco (`PRAGMA user_version = 3`) refez a tabela `anexos` numa transação, com as chaves estrangeiras desligadas só durante a troca e conferidas antes do commit, para aceitar a origem `arquivo` e ganhar `tipo`, `formato` e `nome`; os ids e as linhas ficam iguais. As mensagens, as notas e os pedidos ficam fora da corrente de eventos, em tabelas que podem ser apagadas ([decisão 0006](decisoes/0006-apresentacao-e-historico.md)); a nota grava só um evento com o tamanho do texto. A lousa também fica fora ([decisão 0008](decisoes/0008-lousa-fora-da-corrente.md)): até 2.000 itens por lousa, texto de até 8.000 caracteres (título e rótulo até 120), sem caracteres de controle nem nada que pareça senha ou chave; apagar um item leva as ligações dele, e o cartão de uma tarefa apagada fica sem a tarefa ("Tarefa removida"). A versão 4 do banco (`PRAGMA user_version = 4`) criou as tabelas da lousa e a coluna `anexos.na_lousa`. A versão 5 criou `conexoes_banco` e `consultas_banco`. O histórico de consultas fica fora da corrente (texto até 20.000 caracteres; o SQL que parece levar uma senha, como `CREATE USER … PASSWORD '…'`, roda mas não fica guardado); na corrente vão só `banco.conexao` (`{acao, conexao_id, nome, tipo}`), `banco.consulta` (`{conexao_id, conexao, tipo, verbo, linhas, ms, erro, origem}` e, de um agente, `resultado`) e `banco.alteracao` (`{…, linhas_afetadas}`), sem SQL nem resultado. A conta separada de uma ferramenta num perfil fica em `perfis/<id>/contas/<ferramenta>/`, apontada pela variável da própria ferramenta (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`). As cópias isoladas ficam em `copias/<tarefa>-<branch>/`. Colunas novas entram por migração ao abrir o banco, sem perder o que já está gravado.

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

1. **Fundação:** núcleo e canal, perfis, projetos, branches por worktree, terminais com agentes, eventos em tempo real, linha do tempo em SQLite, quadro por projeto, daily e sprint com modo apresentação, histórico de mensagens por agente, núcleo como servidor MCP para o Claude Code (pedidos pela daily, nota, anexos e navegador da tarefa), arquivos da tarefa, lousa do workspace e da tarefa, conexões de banco por perfil com consultas dos agentes aprovadas por você (Linux e Windows 10/11; no Windows, veja a [decisão 0011](decisoes/0011-windows-com-socket-unix.md)).
2. **Orquestração:** MCP para Codex, Gemini e OpenCode, aprovações em três opções e modo autônomo, receitas como skills, comunicação entre agentes com limite contra loops, notificações e mascote.
3. **Integrações:** servidores MCP de GitHub e Docker por branch, tarefa a partir de link, tela de provedores.
4. **Expansão:** servidores de terceiros, "entender projeto", busca e replay, custos, acesso remoto e celular.
