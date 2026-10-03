# Mudanças

O formato segue [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/) e as versões seguem [SemVer](https://semver.org/lang/pt-BR/).

## [Não lançado]

### Novo

- **Pedir ao agente** pela Daily, pela Sprint e pela apresentação (botão no cartão, menu do botão direito, P no slide): o pedido vai para o Claude Code da tarefa (o ativo, um parado que volta com a conversa ou um novo retomando a última conversa da pasta) e a resposta volta para a nota e para os anexos, sem sair dali. A caixa diz antes para quem vai; a pílula do cartão e a faixa do slide mostram o estado (na fila, aprovar no terminal, com o agente, parou sem responder, respondido, não deu certo). O pedido entra no terminal numa linha só e fica "Respondido" quando o agente chama `concluir_pedido` ou termina a vez; o aviso "O agente respondeu · Ver" fica até ser visto ou fechado.
- **Ferramentas da Colmeia para o agente (MCP):** o próprio núcleo vira servidor MCP do Claude Code que a Colmeia abre, com um token só dele, restrito à tarefa: ler a tarefa e a nota, complementar ou reescrever a nota, anexar uma imagem da pasta, abrir e capturar o navegador e concluir o pedido.
- **Navegador da tarefa:** "Navegador" no painel abre um Chrome ou Chromium controlado pela Colmeia ao lado da janela (perfil próprio, controle só pelo pipe, sem porta), com "Capturar navegador" (Ctrl+Shift+B) anexando a captura à tarefa. Só abre `http`, `https` e `file://` de dentro da pasta, sem `.env` nem chaves (também em redirecionamento, iframe ou imagem); um endereço que não abre mostra o erro no campo. Na Daily, na Sprint e na apresentação, a janela aberta pelo agente fica fora da tela.
- **Arquivos da tarefa** (Ctrl+Shift+E): gaveta por cima do terminal (o tamanho dele não muda) com a árvore da pasta, um nível por vez, e pré-visualização de texto e imagem; abrir no editor, no sistema (só documentos e imagens) e citar na mensagem. `.env`, chaves e certificados pedem confirmação antes de aparecer.
- **Nota editada enquanto o agente complementa:** a tela junta sozinha o que o agente acrescentou; se ele reescreveu, uma janela mostra as duas versões.
- API: `/v1/tarefas/{id}/pedidos` (e `/destino`), `/v1/pedidos/{id}`, `/v1/navegador`, `/v1/tarefas/{id}/navegador` (e `/captura`), `/v1/perfis/{id}/apresentando`, `/v1/tarefas/{id}/arquivos` e `/arquivo`, as rotas `/v1/agente/*` (só com o token de um agente) e `versao` em `PUT /v1/tarefas/{id}/notas`; eventos `pedido.*` e `navegador.*`. Subcomando `colmeia-nucleo mcp`.
- **Histórico de mensagens:** na caixa de mensagem, ↑ traz o que você já mandou ao agente em foco e ↓ volta (o rascunho não se perde; Esc volta a ele). Fica no núcleo, até 200 por agente; o que parece senha ou chave não é guardado, e "Limpar histórico de mensagens" no menu do agente apaga tudo.
- **Linha do tempo nova:** cabeçalho do dia preso no topo com os números do dia, um cartão por tarefa com o estado atual, repetições juntadas ("Anotou na daily (2 vezes)"), miniaturas no cartão (o visor mostra a legenda e a hora da imagem clicada) e filtros (Só conclusões, Só erros, Com capturas).
- **Páginas Daily e Sprint** no lugar do painel lateral: números do período, cartões por tarefa na ordem dos slides, texto para copiar recolhido, tempo por ferramenta e galeria na sprint.
- **Modo apresentação** (F5 ou "Apresentar"): capa com o resumo e um slide por tarefa, com o que foi feito, os números, as fotos e vídeos e uma nota editável; navegação pelo teclado, tela cheia (F11), Esc em camadas, "Novidades · R atualiza" e nenhum aviso durante a apresentação. Os avisos do slide (envio, formato recusado, "Desfazer") ficam numa faixa própria no centro do rodapé.
- **Os mesmos números nas três telas:** Concluídas, Em revisão, Aguardando você, Trabalhando (os nomes dos grupos), Erros (sessões que pararam com erro, como no dia da linha do tempo) e tarefas novas (todas as criadas no período), calculados no núcleo. O estado de cada tarefa tem forma além da cor (ponto cheio, anel, anel grosso para erro), e as pílulas usam um nome só por coluna ("Concluída", "Em revisão").
- **Fotos e vídeos** nos slides: pelo diálogo (A), arrastando para a janela ou com Ctrl+V. O vídeo abre no reprodutor do sistema; a foto JPEG vira PNG sem EXIF.
- API: `/v1/perfis/{id}/apresentacao`, `/v1/tarefas/{id}/notas`, `/v1/tarefas/{id}/videos`, `/v1/anexos/{id}/info` e `/v1/agentes/{id}/mensagens`; evento `nota.atualizada`. A nota que parece ter senha ou chave é recusada, como no histórico de mensagens.

### Mudou

- A tabela `anexos` aceita vídeos e arquivos (`tipo`, `formato`, `nome`; banco na versão 3, migrado sozinho ao abrir).
- O texto do dia anterior na linha do tempo leva a data ("Ontem · quinta, 1 de outubro").
- "O que foi feito" no slide e nos cartões mostra trabalho, não eventos de sistema: anexos e capturas ficam só como mídia, e as sessões de um agente viram uma linha ("Claude Code (dev) trabalhou 1h10 em 7 sessões").
- Na capa, o destaque começa pelo verbo ("Esperando minha resposta: …") e cada título é cortado em cerca de 40 caracteres; as tarefas das colunas têm até 2 linhas e a palavra do estado.
- O subtítulo da sprint diz a duração e o escopo ("14 dias · cliente-x e loja-web"); o ano só aparece quando o período cruza a virada do ano.
- O cabeçalho da tarefa corta o título, nunca a etiqueta de estado nem os botões.
- Todo texto cortado termina em "…", sem espaço nem vírgula antes.
- A cor de alerta do tema Leitura ficou mais amarela, para se separar do destaque e do erro.
- O slide usa o espaço livre para a nota (até 14 linhas) e, depois de um pedido respondido, o cartão e o slide mostram o fim da nota, onde está a resposta.

### Corrigido

- O menu "Branch" em "Todos os projetos" não mostra mais um item vazio vindo de uma pasta sem git.
- Uma nota mudada por fora (outra tela, a API) durante a apresentação acende "Novidades"; as notas e os anexos da própria apresentação, não.
- A barra de rolagem não cobre mais o texto nem a borda dos cartões.
- Reticências duplicadas ("texto.……") no corte de uma linha que termina em ponto.
- O cursor de texto não pisca mais: a tela parada com a caixa de mensagem em foco não redesenha.

## [0.2.0] · 02/10/2026

### Novo

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

### Mudou

- O núcleo grava o perfil, o projeto, a tarefa e o agente de cada evento (dentro do hash e em colunas indexadas) e o título nas mudanças de tarefa. Os eventos antigos são ligados aos perfis na primeira abertura.
- A regra "agente começou numa tarefa do Backlog: vai para Trabalhando" saiu da tela e foi para o núcleo.
- Imagens coladas e capturas são guardadas pelo núcleo (decodificadas e codificadas de novo, sem metadados, `0600`).
- O terminal da tela dorme no `poll(2)` em vez de acordar 200 vezes por segundo: com 10 agentes parados, a tela parada foi de 9% para 0% de processador.
- Os temas claro e leitura passaram a ter os mesmos tamanhos e espaçamentos do escuro; as cores de estado ganharam contraste.
- `scripts/medir.sh --pid` mede um processo exato.

### Corrigido

- Um núcleo iniciado pela tela que caía ficava como processo zumbi até a tela fechar.

## [0.1.0] · 02/10/2026

- Perfis, contas de IA, workspaces, projetos (git ou pasta), tarefas e agentes de verdade, cópias isoladas por git worktree, retomar conversa do Claude Code, teclado completo no terminal.
