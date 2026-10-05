# Mudanças

O formato segue [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/) e as versões seguem [SemVer](https://semver.org/lang/pt-BR/). A Colmeia foi construída em entregas (A a G); dentro de cada versão, as mudanças vêm agrupadas pela entrega que as trouxe, da mais nova para a mais antiga.

## [Não lançado]

### Entrega H · Enter envia, daily e sprint gerais e tempo dos agentes só quando pedido

#### Novo

- **Visão do workspace:** o nome do workspace na barra lateral é clicável e mostra todos os projetos dele no quadro, na linha do tempo, na daily e na sprint, sem navegar projeto por projeto. "Todos os projetos" continua sendo o perfil inteiro.
- **Daily e sprint gerais, separadas por projeto:** no workspace ou no perfil, os cartões vêm por projeto (na ordem da barra lateral), com uma fileira de saltos no topo ("loja-web · 4"), os links **Só este projeto** e **Abrir quadro** em cada projeto e o rodapé "Sem atividade no período: …". Dois projetos com o mesmo nome em workspaces diferentes aparecem com o workspace na frente. Na visão de um projeto, "Ver o workspace X inteiro". O texto da daily sai por projeto e a apresentação ganha um divisor por projeto também na daily (com o workspace em cima quando há mais de um).
- **Chip "Ver"** na barra do registro: o perfil, cada workspace e os projetos de cada um, num menu só; muda o mesmo escopo da barra lateral.
- **Recolher workspace na barra lateral:** a seta ao lado do nome esconde a lousa e os projetos dele (o projeto aberto continua visível). Recolhido, um ponto ao lado do nome mostra o estado mais urgente dos projetos escondidos, para nenhum agente esperando passar despercebido. Fica guardado no núcleo e vale para todas as telas do perfil.
- **Mostrar tempo dos agentes:** caixa na barra do registro (no "⋯" quando a barra aperta), guardada por perfil no núcleo.
- API: `workspace=` na linha do tempo, no resumo e na apresentação (exclusivo com `projeto=`); `tempo_agentes` no `PATCH /v1/perfis/{id}`, no perfil e nas respostas do registro; `secao_id` e `workspace` nos slides, `projeto_id` e `workspace` nas seções da sprint; evento `perfil.tempo_agentes`.

#### Mudou

- **Enter envia a mensagem** na caixa do painel da tarefa e em "Pedir ao agente", como no terminal do Claude Code; **Shift+Enter quebra a linha**. Ctrl+Enter continua enviando; Alt+Enter e o Enter de uma composição de acento não fazem nada; colar texto com várias linhas não envia; com um diálogo aberto, o Enter é do diálogo. O console SQL, a lousa, o texto da daily e a nota do slide continuam com Ctrl+Enter (a dica da nota agora diz).
- **O tempo dos agentes fica escondido por padrão** na daily, na sprint, na linha do tempo, na apresentação e no texto copiado (o núcleo nem manda os tempos); os tempos continuam gravados ([decisão 0010](docs/decisoes/0010-tempo-dos-agentes-opcional.md)). O banco ganha a coluna `perfis.tempo_agentes`, migrada sozinha.
- Com o tempo desligado, a fileira de números não mostra "Tempo de agente" (antes aparecia "0").
- **Slide da apresentação com a nota em destaque:** "O que foi feito" começa recolhido ("O que foi feito · 6 itens", um clique abre) e a nota ganha a fonte maior da coluna, com o markdown formatado (títulos, negrito, código, listas e tabelas simples) em vez dos símbolos crus.
- API: `PATCH /v1/workspaces/{id}` com `recolhido`, `workspace_recolhido` nos projetos e o evento `workspace.recolhido`.

### Entrega G · Bancos de dados por perfil

#### Novo

- **Bancos de dados por perfil** ("Bancos de dados" na barra lateral, Ctrl+Shift+K): conexões de PostgreSQL, MySQL/MariaDB, SQL Server (experimental) e SQLite, em pastas, com Testar ("Conectou em 42 ms · PostgreSQL 16.4 · com TLS" ou o erro em frase simples), TLS configurável e as opções "Permitir alterações" e "Agentes podem pedir consultas". A senha vai para o chaveiro do sistema; sem chaveiro, fica só na memória e é pedida ao conectar. A falta do chaveiro (bloqueado, fora do ar) vale só para a sessão: quando ele volta, a senha guardada nele é usada de novo. Senha recusada pelo servidor (trocada lá, ou conexão salva sem senha): clicar no erro da árvore abre o diálogo de senha, e o erro do console traz "Trocar senha…" (que refaz o que falhou: a instrução ou a prévia da tabela) e "Editar conexão". Senha certa sem acesso ao banco (MySQL 1044) diz "Sem permissão no banco loja." e não oferece trocar a senha.
- **Árvore** conexão › banco › esquema › Tabelas/Views › colunas (PK e FK), carregada aos poucos, com filtro (Ctrl+F), contagem ("12 de 1007"), prévia das 100 primeiras linhas no clique duplo e "Gerar SELECT".
- **Console por conexão:** realce do SQL, Ctrl+Enter na seleção ou na instrução sob o cursor, grade virtual com cabeçalho preso, números alinhados pelo separador decimal, binários como "<binário 2 bytes>", colunas ajustáveis, cópia em TSV (Ctrl+C, Ctrl+A e o menu), limite de linhas com "Carregar mais", tempo-limite, Cancelar (Esc, com o foco voltando ao editor) e o histórico de consultas da conexão (o clique põe a instrução no editor, separada por uma linha em branco). A barra do console e a trilha dizem em qual conexão o Ctrl+Enter roda.
- **Só leitura por padrão;** com "Permitir alterações", cada alteração pede confirmação com a instrução inteira e o aviso de UPDATE ou DELETE sem WHERE.
- **O agente consulta o banco com a sua aprovação:** ferramentas MCP `listar_bancos` e `consultar_banco`. O pedido aparece no painel da tarefa (aprovar, recusar com motivo e Enter, senha quando falta; senha errada não encerra o pedido, o cartão pede de novo), o cartão fica em "Quer consultar o banco", a barra lateral conta os pedidos, um aviso chega uma vez e, sem resposta em 5 min, o pedido expira.
- **Linha do tempo:** "Consultou o banco loja-web-dev (14 vezes)", "Alterou o banco X: UPDATE, 12 linhas" (o clique abre os bancos nessa conexão) e, na tarefa, "Claude Code (dev) consultou o banco X (aprovado por você)", "Você recusou…" e "…expirou sem resposta", sem SQL nem resultado.
- API: `/v1/perfis/{id}/conexoes` (e `/testar`), `/v1/conexoes/{id}` (e `/senha`, `/testar`, `/desconectar`, `/arvore`, `/previa`, `/execucoes`, `/historico`), `/v1/execucoes/{ficha}` (e `/mais`), `/v1/perfis/{id}/aprovacoes`, `/v1/aprovacoes/{id}`, `/v1/agente/bancos` e `/v1/agente/bancos/{id}/consultas`; eventos `banco.conexao`, `banco.consulta` e `banco.alteracao`, avisos `conexao.mudou`, `banco.consulta` e `banco.aprovacao`; `aprovacoes` no `/quadro`; motivo `aprovar consulta` no estado do agente.

#### Mudou

- O banco vai para a versão 5 (tabelas `conexoes_banco` e `consultas_banco`), migrado sozinho ao abrir.
- O servidor MCP atende cada chamada na sua goroutine, com prazo por ferramenta (60 s; 6 min na consulta ao banco) e `notifications/cancelled`; a configuração MCP pede ao Claude Code um `timeout` de 7 min.
- Ctrl+Shift+P põe os pedidos de consulta antes dos erros (vencem em minutos).
- Os botões secundário e de alerta mostram o contorno do foco do teclado.
- A linha do tempo ordena os itens pela hora que mostram (consultas juntadas ficam na hora da última; "esperando você desde 14:26" fica às 14:26).

#### Corrigido

- Numa tarefa sem agente, o "+ Agente" do cabeçalho sumia e a lousa ligada escondia o caminho para abrir um agente. Agora o "+ Agente" aparece sempre. Com a lousa ligada, uma faixa acima dela mantém "Adicionar agente", "Retomar conversa do Claude Code" e "Fechar lousa" à vista, e a lousa não volta ligada ao abrir a tarefa de novo. O estado vazio explica que o agente abre ali, com a caixa de mensagem embaixo.
- Um núcleo que encerrava depois de outro já ter subido apagava o socket e o token do novo, que ficava rodando sem canal.

### Entrega F · Lousa

#### Novo

- **Lousa (quadro livre)** por workspace (na barra lateral) e por tarefa (chip "Lousa", Ctrl+Shift+Q, por cima do terminal sem mudar o tamanho dele): notas em markdown, texto solto, trechos de código, imagens (Ctrl+V, arrastar ou escolher), vídeos, cartões de tarefa ao vivo e ligações tracejadas com rótulo. Clique duplo cria uma nota em edição, a altura cresce com o texto, puxar a borda liga itens (soltar no vazio cria uma nota ligada), seleção por caixa e Shift, duplicar, copiar e colar entre lousas, cores, desfazer e refazer, zoom por níveis com Ctrl+rodinha e mover a vista pelo fundo, espaço ou botão do meio. Gravação em lotes (texto 800 ms depois da última tecla), conflito por versão como na nota da daily ("Mudou em outra tela; atualizei") e texto que parece senha ou chave recusado.
- **O agente na lousa da tarefa:** ferramentas MCP `ler_lousa` e `acrescentar_a_lousa` (só acrescenta; posiciona sozinho), com o item novo marcado e "Claude Code acrescentou 6 itens · Ver" quando cai fora da vista; na linha do tempo, "Claude Code (dev) acrescentou 6 itens à lousa", que abre a tarefa com a lousa enquadrada.
- **Lousa na apresentação:** aba "Lousa" no slide da tarefa (L alterna com os anexos), só leitura e ajustada para caber; o clique (ou F5 na lousa) abre o palco, em tela cheia, de cartão em cartão seguindo as ligações, com visão geral (O) e o vídeo abrindo no reprodutor (clique no play ou Enter).
- API: `/v1/workspaces/{id}/lousa`, `/v1/tarefas/{id}/lousa`, `/v1/lousas/{id}` e `/v1/lousas/{id}/operacoes`, `/v1/perfis/{id}/videos` e `lousa=1` nos anexos do perfil, `/v1/agente/lousa` e `/v1/agente/lousa/elementos`; aviso `lousa.mudou`, evento `lousa.agente` e `lousa` em cada slide do deck.

#### Mudou

- O banco vai para a versão 4 (tabelas `lousas` e `lousa_elementos`, coluna `anexos.na_lousa`), migrado sozinho ao abrir.
- A caixa de mensagem só pega o teclado depois do clique que pediu (antes, um botão que punha texto nela perdia o foco no mesmo quadro).
- O filtro de segredos (histórico de mensagens, notas da daily e lousa) também pega nomes com senha, password, secret, token ou key em qualquer posição (`db_password=…`, `SENHA_DB=…`) e a frase "a senha … é …", quando o valor tem algarismo ou símbolo.
- Os chips de alternar ("Arquivos", "Lousa", "Visão geral") têm a mesma largura ligados e desligados.

### Entrega E · Pedir ao agente, núcleo como servidor MCP, navegador e arquivos da tarefa

#### Novo

- **Pedir ao agente** pela Daily, pela Sprint e pela apresentação (botão no cartão, menu do botão direito, P no slide): o pedido vai para o Claude Code da tarefa (o ativo, um parado que volta com a conversa ou um novo retomando a última conversa da pasta) e a resposta volta para a nota e para os anexos, sem sair dali. A caixa diz antes para quem vai; a pílula do cartão e a faixa do slide mostram o estado (na fila, aprovar no terminal, com o agente, parou sem responder, respondido, não deu certo). O pedido entra no terminal numa linha só e fica "Respondido" quando o agente chama `concluir_pedido` ou termina a vez; o aviso "O agente respondeu · Ver" fica até ser visto ou fechado.
- **Ferramentas da Colmeia para o agente (MCP):** o próprio núcleo vira servidor MCP do Claude Code que a Colmeia abre, com um token só dele, restrito à tarefa: ler a tarefa e a nota, complementar ou reescrever a nota, anexar uma imagem da pasta, abrir e capturar o navegador e concluir o pedido.
- **Navegador da tarefa:** "Navegador" no painel abre um Chrome ou Chromium controlado pela Colmeia ao lado da janela (perfil próprio, controle só pelo pipe, sem porta), com "Capturar navegador" (Ctrl+Shift+B) anexando a captura à tarefa. Só abre `http`, `https` e `file://` de dentro da pasta, sem `.env` nem chaves (também em redirecionamento, iframe ou imagem); um endereço que não abre mostra o erro no campo. Na Daily, na Sprint e na apresentação, a janela aberta pelo agente fica fora da tela.
- **Arquivos da tarefa** (Ctrl+Shift+E): gaveta por cima do terminal (o tamanho dele não muda) com a árvore da pasta, um nível por vez, e pré-visualização de texto e imagem; abrir no editor, no sistema (só documentos e imagens) e citar na mensagem. `.env`, chaves e certificados pedem confirmação antes de aparecer.
- **Nota editada enquanto o agente complementa:** a tela junta sozinha o que o agente acrescentou; se ele reescreveu, uma janela mostra as duas versões.
- API: `/v1/tarefas/{id}/pedidos` (e `/destino`), `/v1/pedidos/{id}`, `/v1/navegador`, `/v1/tarefas/{id}/navegador` (e `/captura`), `/v1/perfis/{id}/apresentando`, `/v1/tarefas/{id}/arquivos` e `/arquivo`, as rotas `/v1/agente/*` (só com o token de um agente) e `versao` em `PUT /v1/tarefas/{id}/notas`; eventos `pedido.*` e `navegador.*`. Subcomando `colmeia-nucleo mcp`.

#### Mudou

- O slide usa o espaço livre para a nota (até 14 linhas) e, depois de um pedido respondido, o cartão e o slide mostram o fim da nota, onde está a resposta.

#### Corrigido

- Reticências duplicadas ("texto.……") no corte de uma linha que termina em ponto.
- O cursor de texto não pisca mais: a tela parada com a caixa de mensagem em foco não redesenha.

### Entrega D · Histórico de mensagens e modo apresentação

#### Novo

- **Histórico de mensagens:** na caixa de mensagem, ↑ traz o que você já mandou ao agente em foco e ↓ volta (o rascunho não se perde; Esc volta a ele). Fica no núcleo, até 200 por agente; o que parece senha ou chave não é guardado, e "Limpar histórico de mensagens" no menu do agente apaga tudo.
- **Linha do tempo nova:** cabeçalho do dia preso no topo com os números do dia, um cartão por tarefa com o estado atual, repetições juntadas ("Anotou na daily (2 vezes)"), miniaturas no cartão (o visor mostra a legenda e a hora da imagem clicada) e filtros (Só conclusões, Só erros, Com capturas).
- **Páginas Daily e Sprint** no lugar do painel lateral: números do período, cartões por tarefa na ordem dos slides, texto para copiar recolhido, tempo por ferramenta e galeria na sprint.
- **Modo apresentação** (F5 ou "Apresentar"): capa com o resumo e um slide por tarefa, com o que foi feito, os números, as fotos e vídeos e uma nota editável; navegação pelo teclado, tela cheia (F11), Esc em camadas, "Novidades · R atualiza" e nenhum aviso durante a apresentação. Os avisos do slide (envio, formato recusado, "Desfazer") ficam numa faixa própria no centro do rodapé.
- **Os mesmos números nas três telas:** Concluídas, Em revisão, Aguardando você, Trabalhando (os nomes dos grupos), Erros (sessões que pararam com erro, como no dia da linha do tempo) e tarefas novas (todas as criadas no período), calculados no núcleo. O estado de cada tarefa tem forma além da cor (ponto cheio, anel, anel grosso para erro), e as pílulas usam um nome só por coluna ("Concluída", "Em revisão").
- **Fotos e vídeos** nos slides: pelo diálogo (A), arrastando para a janela ou com Ctrl+V. O vídeo abre no reprodutor do sistema; a foto JPEG vira PNG sem EXIF.
- API: `/v1/perfis/{id}/apresentacao`, `/v1/tarefas/{id}/notas`, `/v1/tarefas/{id}/videos`, `/v1/anexos/{id}/info` e `/v1/agentes/{id}/mensagens`; evento `nota.atualizada`. A nota que parece ter senha ou chave é recusada, como no histórico de mensagens.

#### Mudou

- A tabela `anexos` aceita vídeos e arquivos (`tipo`, `formato`, `nome`; banco na versão 3, migrado sozinho ao abrir).
- O texto do dia anterior na linha do tempo leva a data ("Ontem · quinta, 1 de outubro").
- "O que foi feito" no slide e nos cartões mostra trabalho, não eventos de sistema: anexos e capturas ficam só como mídia, e as sessões de um agente viram uma linha ("Claude Code (dev) trabalhou 1h10 em 7 sessões").
- Na capa, o destaque começa pelo verbo ("Esperando minha resposta: …") e cada título é cortado em cerca de 40 caracteres; as tarefas das colunas têm até 2 linhas e a palavra do estado.
- O subtítulo da sprint diz a duração e o escopo ("14 dias · cliente-x e loja-web"); o ano só aparece quando o período cruza a virada do ano.
- O cabeçalho da tarefa corta o título, nunca a etiqueta de estado nem os botões.
- Todo texto cortado termina em "…", sem espaço nem vírgula antes.
- A cor de alerta do tema Leitura ficou mais amarela, para se separar do destaque e do erro.

#### Corrigido

- O menu "Branch" em "Todos os projetos" não mostra mais um item vazio vindo de uma pasta sem git.
- Uma nota mudada por fora (outra tela, a API) durante a apresentação acende "Novidades"; as notas e os anexos da própria apresentação, não.
- A barra de rolagem não cobre mais o texto nem a borda dos cartões.

### Entre as entregas C e D

- **Terminal do agente nasce no tamanho da tela:** criar e iniciar o agente levam o tamanho do terminal em foco, e só ele muda o tamanho no núcleo (as miniaturas mostram a mesma grade com letra menor). Antes, uma conversa retomada aparecia embaralhada.
- **Rodinha do mouse no Claude Code:** em tela cheia, a rodinha vai para o programa (SGR ou o formato antigo do xterm) e, sem mouse, vira setas; a rolagem suave soma até completar uma linha.
- O núcleo tira do ambiente dos agentes as variáveis de sessão de um Claude Code (com a Colmeia aberta de dentro de um, o agente não guardava a conversa).

## [0.2.0] · 02/10/2026

### Entrega C · Eventos em tempo real, linha do tempo, daily e sprint

#### Novo

- **Eventos em tempo real.** A tela recebe do núcleo, por um WebSocket por perfil, tudo o que muda: tarefas criadas, movidas e removidas, agentes que começam, param ou esperam você. Nada de consulta periódica.
- **Estado de cada agente:** trabalhando, "parece pedir aprovação", "sua vez", parado, terminou, interrompido ou com erro, com a hora fixa ("desde 14:32"). O mesmo texto e a mesma cor no cartão, no painel, na abelha e na linha do tempo.
- **O cartão anda sozinho** entre "Agente trabalhando" e "Aguardando você". Se você mover o cartão, o núcleo não desfaz.
- **Avisos:** um agente fora da tela que precisa de você aparece no rodapé com "Abrir"; com a janela sem foco, o título vira "Colmeia · 2 esperando você" e a janela pede atenção.
- **Linha do tempo** por perfil ou por projeto, dia a dia, com as sessões dos agentes e as capturas.
- **Daily e sprint** prontas para falar: texto editável, Copiar, Copiar em Markdown e Salvar com as capturas.
- **Capturar terminal** (Ctrl+Shift+S): a imagem fica anexada à tarefa, com "Desfazer".
- **Atalhos:** Ctrl+Shift+L (linha do tempo), Ctrl+Shift+D (daily), Ctrl+Shift+P (próximo que precisa de você), Ctrl+Shift+S (capturar).
- **Faixa "Núcleo desconectado"** com reconexão sozinha; "Tentar agora" inicia o núcleo de novo se ele tiver caído; sem o núcleo, nada é pedido.
- **Ao fechar com agentes rodando**, a Colmeia pergunta: fechar só a janela ou parar todos.
- `colmeia-nucleo --encerrar` e `--versao`; `scripts/instalar.sh` (sem sudo), atalho e ícone; release por tag no CI.
- Botões "⋯" visíveis no cartão, no projeto e no agente; "+ Nova tarefa" em "Todos os projetos", com a escolha do projeto; Enter confirma e Esc cancela em todos os diálogos.

#### Mudou

- O núcleo grava o perfil, o projeto, a tarefa e o agente de cada evento (dentro do hash e em colunas indexadas) e o título nas mudanças de tarefa. Os eventos antigos são ligados aos perfis na primeira abertura.
- A regra "agente começou numa tarefa do Backlog: vai para Trabalhando" saiu da tela e foi para o núcleo.
- Imagens coladas e capturas são guardadas pelo núcleo (decodificadas e codificadas de novo, sem metadados, `0600`).
- O terminal da tela dorme no `poll(2)` em vez de acordar 200 vezes por segundo: com 10 agentes parados, a tela parada foi de 9% para 0% de processador.
- Os temas claro e leitura passaram a ter os mesmos tamanhos e espaçamentos do escuro; as cores de estado ganharam contraste.
- `scripts/medir.sh --pid` mede um processo exato.

#### Corrigido

- Um núcleo iniciado pela tela que caía ficava como processo zumbi até a tela fechar.

## [0.1.0] · 02/10/2026

### Entrega B · Agentes de verdade, cópias isoladas e pastas sem git

- Pastas de trabalho sem git entram como projeto (sem branches).
- Tarefa numa cópia isolada (git worktree), numa branch nova ou existente, ou direto na pasta; remover recusa uma cópia com mudanças fora de commit.
- Agentes por tarefa (Claude Code, Codex, Gemini CLI, OpenCode ou o shell) abertos na pasta da tarefa com a conta de IA do perfil, em terminais que sobrevivem à tela.
- Conversas do Claude Code da pasta listadas com título e retomadas com `--resume`; cada agente novo nasce com um id de conversa, para voltar na mesma.
- Painel da tarefa com "Adicionar agente", "Retomar conversa", agente parado com o botão para voltar, "Abrir no IntelliJ" e "Abrir pasta"; teclado completo no terminal.

### Entrega A · Perfis, contas de IA, projetos e tarefas de verdade

- Banco local (pastas `0700`, arquivos `0600`) com perfis, contas de IA, workspaces, projetos e tarefas, e o histórico de eventos encadeado por hash, conferido ao iniciar.
- API `/v1` com validação no núcleo: pasta do projeto conferida com o git (sem shell), nomes de branch perigosos recusados, JSON estrito e erros em português.
- Ferramentas de agente instaladas detectadas; conta separada por perfil para o Claude Code e o Codex.
- Entrada por perfil e assistente em três passos (nome e tema, contas de IA, primeiro projeto); tema Leitura; caixa de mensagem com Enter, Ctrl+Enter e Ctrl+V para imagens.

### Fundação

- Núcleo em Go dono dos terminais, com controle de fluxo, ritmo por conexão e histórico de tamanho fixo; canal local sem porta de rede (socket Unix do usuário e token por início); protocolo em `/v1`; tela em Rust com egui; a abelha-robô como biblioteca.
