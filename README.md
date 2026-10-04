# Colmeia

Um app desktop para coordenar vários agentes de IA de código (Claude Code, Codex e outros) num lugar só: um quadro de tarefas por projeto, o terminal de cada agente no painel da tarefa, uma abelha que avisa quando algo precisa de você e o registro do trabalho pronto para a daily e a sprint. Roda no Linux.

![O quadro de um projeto: um agente escrevendo testes, outro esperando a sua resposta e as tarefas em revisão e concluídas](docs/imagens/quadro.png)

Quem trabalha com vários agentes vira o gargalo: copia contexto de um terminal para outro, perde o controle do que cada um está fazendo e mistura ambientes. A Colmeia junta:

- **Isolamento por branch:** cada tarefa pode trabalhar numa cópia isolada do repositório (git worktree), na branch dela. Pastas sem git (análises, anotações) também entram como projeto.
- **Quadro por projeto**, com cartões que andam sozinhos entre "Agente trabalhando" e "Aguardando você".
- **A abelha**, que resume o que mais precisa de você (erro, aprovação pendente, trabalho em andamento) e comemora quando uma tarefa termina.
- **Perfis separados** (Profissional, Estudo, Pessoal…), cada um com projetos, contas de IA, conexões de banco e tema próprios.
- **Registro do trabalho** dia a dia, com a daily e a sprint prontas para falar ou apresentar.

![A abelha-robô da Colmeia nos cinco estados: dormindo, trabalhando, aguardando você, bugado e comemorando](docs/imagens/abelha.gif)

É um projeto pessoal, de código aberto e gratuito. Não é projeto de nenhuma empresa nem tem vínculo com uma.

## Conteúdo

- [Instalação](#instalação)
- [Primeiro uso](#primeiro-uso)
- [Funcionalidades](#funcionalidades): [quadro e agentes](#quadro-e-agentes) · [linha do tempo, daily, sprint e apresentação](#linha-do-tempo-daily-sprint-e-apresentação) · [pedir ao agente, navegador e arquivos](#pedir-ao-agente-navegador-e-arquivos) · [lousa](#lousa) · [banco de dados](#banco-de-dados)
- [Atalhos](#atalhos)
- [Arquitetura](#arquitetura) · [Segurança](#segurança)
- [Desenvolvimento e testes](#desenvolvimento-e-testes)
- [Plataformas](#plataformas) · [Licença](#licença)

## Instalação

Cada versão publicada tem os binários para Linux x86_64 na página de releases do GitHub (`colmeia-linux-x86_64.tar.gz`, com o `SHA256SUMS` ao lado):

```bash
sha256sum -c SHA256SUMS
mkdir colmeia && tar -xzf colmeia-linux-x86_64.tar.gz -C colmeia
./colmeia/instalar.sh   # ~/.local/bin, atalho e ícone; sem sudo
```

Para compilar a partir do código, veja [Desenvolvimento e testes](#desenvolvimento-e-testes).

A Colmeia tem duas partes: a **tela** (a janela) e o **núcleo**, um processo em segundo plano que é dono dos terminais. A tela inicia o núcleo sozinha. Por isso, **fechar a janela não para os agentes**, que seguem trabalhando (e usando a conta de IA); ao fechar com agentes rodando, a Colmeia pergunta se é para fechar só a janela ou parar todos. Para encerrar o núcleo e os agentes (que têm uns segundos para salvar a conversa): `colmeia-nucleo --encerrar`.

Os dados ficam em `~/.local/share/colmeia/`, só com acesso do seu usuário.

## Primeiro uso

1. **Criar perfil:** nome (Profissional, Estudo, Pessoal…) e tema (escuro, claro ou leitura).
2. **Contas de IA:** a Colmeia mostra as ferramentas instaladas (Claude Code, Codex, Gemini CLI, OpenCode). Cada uma pode usar a conta do sistema ou uma conta só daquele perfil; nesse caso, o login é feito na primeira vez que o agente abrir.
3. **Primeiro projeto:** um repositório git ou qualquer pasta de trabalho. Adicionar não altera nada na pasta.
4. **Falar com o agente:** "+ Nova tarefa" cria uma tarefa no Backlog. Clique nela e use **"Adicionar agente"** (ou "+ Agente", no topo do painel da tarefa): o terminal do Claude Code abre ali, e a caixa de mensagem embaixo manda o que você escrever (Ctrl+Enter).

O perfil se troca pelo seletor no topo da barra lateral. Arrastar move o cartão entre colunas; o "⋯" (ao passar o mouse) ou o botão direito no cartão, no projeto e no agente mostram as outras ações.

## Funcionalidades

### Quadro e agentes

- **Onde o agente trabalha.** Numa tarefa de repositório git você escolhe entre uma **cópia isolada** (git worktree numa branch nova ou existente, em `~/.local/share/colmeia/copias/`) e **direto na pasta** do projeto. Numa pasta sem git, direto na pasta.
- **Adicionar agente** abre a ferramenta escolhida no painel da tarefa com a conta de IA do perfil. Vários agentes por tarefa: o terminal em foco aparece em tempo real, os outros como miniaturas.
- **Retomar conversa do Claude Code:** a Colmeia lista as conversas que o Claude Code guardou para a pasta da tarefa e abre a escolhida, para continuar ali uma conversa começada no IntelliJ ou num terminal. Se o Claude Code estiver aberto na mesma pasta fora da Colmeia, ela avisa para fechar lá antes.
- **Um agente parado** (o programa terminou ou o núcleo foi reiniciado) mostra o que ficou na tela e o botão para iniciar de novo; o Claude Code volta na mesma conversa.
- **Teclado completo no terminal** depois de clicar nele (Ctrl, Alt, setas com modificadores, F1–F12, Shift+Enter); Ctrl+V com uma imagem copiada repassa a colagem ao Claude Code.
- **Caixa de mensagem:** Enter quebra a linha, Ctrl+Enter envia, Ctrl+V cola imagens (guardadas e anexadas à tarefa) e ↑ traz as mensagens anteriores, que ficam guardadas (até 200 por agente). Uma mensagem que parece ter senha ou chave não é guardada no histórico; "Limpar histórico de mensagens", no "⋯" do agente, apaga tudo.
- **"Abrir no IntelliJ"** (ou no VS Code) e **"Abrir pasta"** ficam no topo do painel da tarefa.

**Em tempo real, sem consulta periódica.** Cada agente aparece com um estado, com a mesma cor e o mesmo texto no cartão, no painel, na abelha e na linha do tempo:

| Estado | Como aparece |
| --- | --- |
| Trabalhando | ponto verde e a última linha do terminal |
| Pede aprovação | "Parece pedir aprovação · desde 14:32" |
| Sua vez | "Sua vez · desde 14:32" (terminou a resposta e espera você) |
| Quer consultar o banco | um pedido de consulta esperando a sua aprovação |
| Parado | "Parado desde 14:10" (terminal comum quieto) |
| Terminou / interrompido / erro | "Terminou às 14:40", "Interrompido às 14:40", "Parou com erro (código 1) às 14:40" |

- **"Aguardando você" é uma leitura, não uma certeza.** Depois de 5 segundos sem saída, uma ferramenta de IA parece ter terminado a vez dela; se o fim do texto for um pedido de aprovação conhecido, ela "pede aprovação". Por isso a tela diz "parece" ([decisão 0005](docs/decisoes/0005-estado-do-agente-por-heuristica.md)).
- **O cartão anda sozinho** entre "Agente trabalhando" e "Aguardando você" e sai do Backlog quando um agente de IA começa. Se você mover o cartão, a Colmeia não desfaz; nada vai para Revisão ou Concluído sozinho.
- **Quando algo precisa de você** fora da tela, aparece um aviso no rodapé com "Abrir"; com a janela em segundo plano, o título vira "Colmeia · 2 esperando você".
- **Sem o núcleo**, a faixa "Núcleo desconectado" aparece, o quadro continua visível e a tela tenta reconectar sozinha; "Tentar agora" inicia o núcleo de novo.

### Linha do tempo, daily, sprint e apresentação

![A apresentação da daily: o slide de uma tarefa com o que foi feito, os números, a nota e as fotos](docs/imagens/apresentacao.png)

As páginas **Quadro · Linha do tempo · Daily · Sprint** ficam na barra de cima e seguem o escopo da barra lateral (o perfil inteiro ou um projeto).

- **Linha do tempo:** um cabeçalho por dia, preso no topo ao rolar, com as conclusões, os erros e o tempo de agente do dia; dentro dele, um cartão por tarefa com os eventos em lista (repetições viram uma linha, como "Anotou na daily (2 vezes)") e as capturas em miniatura. Filtros: Só conclusões, Só erros e Com capturas.
- **Daily:** os números do período ("ontem" é o último dia com atividade, até 7 dias atrás, e hoje) e um cartão por tarefa, agrupado em concluídas, em revisão, aguardando você, trabalhando e com erro. "Copiar texto" copia o texto pronto para falar.
- **Sprint:** 7 ou 14 dias, este mês ou as datas que você escolher (até 92 dias), com o tempo de agente por ferramenta, as tarefas por projeto e a galeria das capturas; exporta em Markdown, com as capturas numa pasta ao lado.
- **Capturar terminal** (Ctrl+Shift+S): a imagem do terminal em foco fica anexada à tarefa. Na primeira vez a Colmeia avisa que a captura guarda o que está visível, inclusive senhas.

![A linha do tempo: o cabeçalho do dia com os números e um cartão por tarefa, com as miniaturas](docs/imagens/linha-do-tempo.png)

**Modo apresentação** (Apresentar ou F5) passa a daily ou a sprint inteira em tela cheia: uma capa com o resumo do período e um slide por tarefa, com o projeto, o estado, o que foi feito, os números, as fotos e vídeos e a nota. Clicar num cartão da Daily ou da Sprint abre a apresentação naquele slide, em janela, para preparar. Durante a apresentação nenhum aviso aparece (a tela pode estar sendo compartilhada).

- **Notas:** N edita a nota da tarefa, que aparece no slide. Ela é salva ao sair do campo, ao trocar de slide e ao sair; cada daily tem as próprias notas. Uma nota que parece ter senha ou chave não é salva.
- **Fotos e vídeos:** A, arrastar arquivos para a janela ou Ctrl+V anexam ao slide atual (png, jpg, mp4, webm, mkv, mov). O vídeo abre no reprodutor do sistema. Toda foto JPEG vira um PNG novo, sem o EXIF (localização e câmera).
- **Novidades:** o que acontece durante a apresentação não mexe nos slides; "Novidades · R atualiza" refaz o deck no mesmo slide.

![A página da daily, com os números do período e os cartões das tarefas](docs/imagens/daily.png)

### Pedir ao agente, navegador e arquivos

- **Pedir ao agente.** Na Daily, na Sprint (botão no cartão ou botão direito) e no slide (P), você pede algo a mais ao agente de uma tarefa sem sair dali: "traga o total de testes gerados e complemente a nota", "capture prints das telas finalizadas". A caixa diz antes para qual agente vai; o pedido espera o agente terminar a vez e nunca entra no meio do que você digita. A resposta volta para a nota e as capturas viram anexos; "O agente respondeu · Ver" avisa quando terminou.
- **Ferramentas da Colmeia para o agente (MCP).** Todo Claude Code que a Colmeia abre recebe ferramentas restritas à tarefa dele: ler a tarefa e a nota, complementar a nota, anexar uma imagem da pasta, abrir e capturar o navegador da tarefa, ler e acrescentar à lousa, listar e consultar bancos (com a sua aprovação) e concluir o pedido. Tudo aparece na linha do tempo. Codex, Gemini e OpenCode ainda não recebem essas ferramentas.
- **Navegador da tarefa** (no topo do painel): um Chrome ou Chromium da Colmeia ao lado da janela, com perfil próprio (nunca o seu). "Navegador ▾" tem "Ir para endereço…", "Capturar navegador" (Ctrl+Shift+B) e "Fechar navegador". Só abre `http`, `https` e arquivos de dentro da pasta da tarefa, menos `.env` e chaves. Durante a apresentação, a janela aberta pelo agente fica fora da tela.
- **Arquivos** (Ctrl+Shift+E): uma gaveta por cima do terminal com a árvore da pasta, só leitura, e a pré-visualização de texto e imagem; dali dá para abrir no editor ou citar o arquivo na mensagem. `.env`, chaves e certificados pedem confirmação antes de aparecer.

### Lousa

![A lousa do workspace: notas em markdown, um trecho de terminal, uma tabela, um cartão de tarefa e as ligações tracejadas entre eles](docs/imagens/lousa.png)

Um quadro livre para pensar: um por workspace ("Lousa", abaixo do nome do workspace na barra lateral) e um por tarefa (o botão "Lousa" no painel da tarefa, Ctrl+Shift+Q).

- **Itens:** nota em markdown simples (títulos, negrito, listas, caixas, tabelas, blocos de código), texto solto, bloco de código, imagem, vídeo, cartão de tarefa ao vivo (o clique duplo abre a tarefa) e ligações tracejadas com rótulo.
- **Criar e mexer:** clique duplo no vazio cria uma nota; N, T, C, I e K criam no lugar do mouse; puxar o círculo da borda de um item até outro liga os dois. Arrastar move, Shift soma à seleção, Ctrl+rodinha dá zoom, Ctrl+D duplica, Delete apaga, Ctrl+Z desfaz e Ctrl+C/V copiam itens entre lousas.
- **Na tarefa,** a lousa cobre o terminal sem mudar o tamanho dele. Numa tarefa sem agente, uma faixa acima da lousa mantém "Adicionar agente" à vista; "Fechar lousa" volta à tarefa. O que o agente acrescenta chega na hora, marcado como novo.
- **Apresentar** (F5 na lousa) leva os cartões ao palco, em tela cheia, seguindo as ligações. No slide da daily, a lousa da tarefa aparece na aba "Lousa".
- Texto com cara de senha ou chave é recusado ([decisão 0008](docs/decisoes/0008-lousa-fora-da-corrente.md)).

### Banco de dados

![A tela de bancos: a árvore com a conexão loja-web-dev aberta até as colunas da tabela pedidos e o console com um SELECT e a grade de resultado](docs/imagens/banco.png)

"Bancos de dados", na barra lateral (Ctrl+Shift+K), guarda as conexões do perfil: PostgreSQL, MySQL/MariaDB, SQL Server (experimental) e SQLite, agrupadas em pastas se você quiser, com TLS configurável.

- **A senha fica no chaveiro do sistema** (Secret Service, o do GNOME), nunca no banco da Colmeia, em log ou em evento. Sem chaveiro, ela é pedida ao conectar e fica só na memória até fechar a Colmeia; quando o chaveiro volta, a senha guardada nele é usada de novo. Se o servidor recusar a senha (trocada no servidor, por exemplo), clicar no erro da árvore ou em "Trocar senha…" no console pede a nova.
- **Árvore** conexão › banco › esquema › Tabelas e Views › colunas (com PK e FK), carregada aos poucos, com filtro e contagem ("12 de 1007"). O clique duplo numa tabela mostra as 100 primeiras linhas; o botão direito tem "Gerar SELECT", "Testar", "Atualizar", "Editar…" e "Remover…".
- **Console por conexão:** a barra e o topo da tela dizem em qual conexão você está. Ctrl+Enter executa a seleção ou a instrução sob o cursor (até o `;` ou uma linha em branco); o resultado vem numa grade com limite de linhas (500 por padrão, "Carregar mais"), tempo-limite, Esc para cancelar e o histórico de consultas da conexão.
- **Só leitura por padrão.** Toda conexão começa só leitura (transação só de leitura; SQLite aberto só para leitura). Para alterar dados, ligue "Permitir alterações" na conexão; cada alteração ainda pede confirmação mostrando a instrução inteira (e avisa quando um UPDATE ou DELETE não tem WHERE). Prefira um usuário de banco só de leitura: a proteção da Colmeia não substitui a do banco.
- **O agente só consulta, com a sua aprovação.** Numa conexão com "Agentes podem pedir consultas", o Claude Code da tarefa pode pedir uma consulta de leitura. A instrução aparece no painel da tarefa, o cartão fica em "Quer consultar o banco" e nada roda até você aprovar; recusar pode levar um motivo, e sem resposta em 5 minutos o pedido expira. O resultado (até 200 linhas) vai para o agente e para o provedor de IA dele.
- A linha do tempo diz que houve consulta ou alteração ("Consultou o banco loja-web-dev (14 vezes)"), nunca o SQL nem o resultado ([decisão 0009](docs/decisoes/0009-bancos-chaveiro-e-somente-leitura.md)).

## Atalhos

A Colmeia reserva **Ctrl+Shift+letra** para ela: esses atalhos não chegam ao programa do terminal.

| Atalho | Ação |
| --- | --- |
| Ctrl+Shift+P | Abre o próximo agente que precisa de você (pedidos de consulta e erros primeiro) |
| Ctrl+Shift+L / Ctrl+Shift+D | Linha do tempo / Daily (de novo volta ao quadro) |
| Ctrl+Shift+K | Bancos de dados (de novo volta à tela anterior) |
| Ctrl+Shift+S / Ctrl+Shift+B | Captura o terminal em foco / o navegador da tarefa |
| Ctrl+Shift+E / Ctrl+Shift+Q | Arquivos / lousa da tarefa |
| Ctrl+Esc | Volta do painel da tarefa (ou dos bancos) ao quadro |
| Ctrl+Enter / ↑ ↓ (caixa de mensagem) | Envia / mensagens anteriores ao agente em foco |
| F5 / Shift+F5 | Apresenta a daily, a sprint ou a lousa / retoma do último slide (ou do item selecionado) |
| Ctrl+Enter / Esc (console de banco) | Executa a instrução sob o cursor ou a seleção / cancela a consulta |
| Ctrl+F (bancos) | Filtro da árvore |
| Ctrl+C / Ctrl+A (grade de resultado) | Copia a célula ou a linha em TSV / escolhe tudo o que foi carregado |

Na apresentação:

| Tecla | Ação |
| --- | --- |
| → PgDn Espaço Enter / ← PgUp Backspace | Próximo / anterior |
| Home / End / C | Primeiro / último / capa |
| N / A / P | Editar a nota / adicionar foto ou vídeo / pedir ao agente |
| L | Alterna anexos e lousa no slide |
| H / T | Esconder o slide / tema claro ou escuro (só nesta apresentação) |
| R / F11 / ? | Atualizar / tela cheia / atalhos |
| Esc | Fecha o que estiver aberto; senão, sai |

## Arquitetura

```
┌─────────────────────────────┐        ┌────────────────────────────┐
│ app/ (Rust + egui)          │        │ nucleo/ (Go)               │
│ a tela: só mostra e pede    │ ─────▶ │ dono do estado, terminais, │
│                             │  /v1   │ agentes, eventos e bancos  │
└─────────────────────────────┘        └────────────────────────────┘
        canal local: socket Unix (diretório 0700) + token
```

- **Núcleo separado da tela.** A tela só mostra e pede; o núcleo guarda o estado num SQLite com o histórico de eventos encadeado por hash, valida tudo e é dono dos terminais. O protocolo é versionado em `/v1`.
- **Tudo por evento.** Um WebSocket por perfil leva à tela cada mudança na hora; a tela parada não faz pedidos nem redesenha ([decisão 0004](docs/decisoes/0004-eventos-por-websocket.md)). Só o terminal em foco é tempo real; as miniaturas recebem em ritmo menor.
- **O núcleo também é o servidor MCP dos agentes**, por stdio, e controla o navegador da tarefa por pipe ([decisão 0007](docs/decisoes/0007-nucleo-mcp-e-navegador-por-pipe.md)).

Os detalhes estão em [docs/arquitetura.md](docs/arquitetura.md), nas [decisões registradas](docs/decisoes/) e na [lista de dependências](docs/dependencias.md).

## Segurança

- **Nenhuma porta de rede.** O núcleo escuta num socket Unix dentro de um diretório só do usuário, com um token gerado a cada início; cada agente tem um token próprio, que só vale para a tarefa dele; o navegador é controlado por pipe.
- **Nada passa por shell**, os dados ficam com permissão só do usuário (`0700`/`0600`) e o núcleo recusa JSON com campos desconhecidos.
- **Segredos:** as senhas dos bancos ficam no chaveiro do sistema, nunca no banco da Colmeia, em log, evento ou resposta; mensagens, notas e itens da lousa que parecem ter senha ou chave não são guardados.
- **Agentes com limites:** o agente só lê nos bancos, e cada consulta precisa da sua aprovação.

O modelo completo, com o que a Colmeia protege e o que não, está em [SECURITY.md](SECURITY.md).

## Desenvolvimento e testes

Requisitos: Go 1.26+, Rust 1.88+ e as bibliotecas de sistema que o `eframe` usa (no Ubuntu, `libxkbcommon-dev`, `libgtk-3-dev` e os drivers de vídeo).

```bash
# Núcleo
go -C nucleo build -o ../bin/colmeia-nucleo ./cmd/colmeia-nucleo

# Tela (inicia o núcleo sozinha se ele não estiver rodando)
COLMEIA_NUCLEO=$PWD/bin/colmeia-nucleo cargo run --release -p colmeia

# Modo demonstração: cargas de teste nos terminais e cenários para a abelha
COLMEIA_DEMO=1 COLMEIA_NUCLEO=$PWD/bin/colmeia-nucleo cargo run --release -p colmeia

# Vitrine do mascote
cargo run --release -p mascote --example vitrine

# Instalar o que foi compilado (~/.local/bin, atalho e ícone)
./scripts/instalar.sh
```

Testes e verificações (o CI roda os mesmos, mais `govulncheck`, `cargo audit` e o gitleaks):

```bash
go -C nucleo test -race ./...
go -C nucleo vet ./... && gofmt -l nucleo
cargo test --workspace
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings

# Integração com MySQL e PostgreSQL de verdade, em contêineres de teste
docker run -d --name colmeia-teste-mysql -p 127.0.0.1:33061:3306 -e MYSQL_ROOT_PASSWORD=teste-raiz -e MYSQL_DATABASE=loja mysql:8
docker run -d --name colmeia-teste-pg -p 127.0.0.1:54321:5432 -e POSTGRES_PASSWORD=teste-raiz -e POSTGRES_DB=loja postgres:16-alpine
COLMEIA_TESTE_MYSQL=127.0.0.1:33061 COLMEIA_TESTE_PG=127.0.0.1:54321 go -C nucleo test -tags integracao ./internal/bancos/
docker rm -f colmeia-teste-mysql colmeia-teste-pg
```

Para mexer na tela sem tocar nos seus dados de verdade, use pastas de teste (`COLMEIA_DIR` e `COLMEIA_DADOS`), como explica o [CONTRIBUTING.md](CONTRIBUTING.md). As mudanças de cada versão estão no [CHANGELOG.md](CHANGELOG.md).

### Variáveis

| Variável | Quem lê | Para quê |
| --- | --- | --- |
| `COLMEIA_DIR` | núcleo e tela | Pasta do canal (socket e token); padrão `$XDG_RUNTIME_DIR/colmeia` |
| `COLMEIA_DADOS` | núcleo | Pasta dos dados; padrão `~/.local/share/colmeia` |
| `COLMEIA_NUCLEO` | tela | Caminho do `colmeia-nucleo`, se ele não estiver ao lado da tela nem no PATH |
| `COLMEIA_DEMO=1` | tela | Inicia o núcleo em modo demonstração |
| `COLMEIA_EDITOR` | tela | Comando do editor para "Abrir no…" |
| `COLMEIA_NAVEGADOR` | núcleo | Chrome ou Chromium do navegador da tarefa (caminho ou nome no PATH) |
| `COLMEIA_CHAVEIRO=memoria` | núcleo | Age como se não houvesse chaveiro: as senhas dos bancos ficam só na memória |
| `COLMEIA_CHAVEIRO_SERVICO` | núcleo | Nome do serviço das senhas no chaveiro; padrão `Colmeia` (os testes usam outro) |
| `COLMEIA_TEMA` | tela | `escuro`, `claro` ou `leitura`, só antes de entrar num perfil |
| `COLMEIA_SEM_ABELHA=1` | tela | Desliga a abelha |
| `COLMEIA_TAMANHO` | tela | Tamanho inicial da janela, como `1280x720` |
| `COLMEIA_FPS=1` | tela | Mostra o contador de quadros e escreve cada quadro no stderr (para medir) |
| `COLMEIA_TAREFA`, `COLMEIA_CARTOES`, `COLMEIA_CENARIO=erro` | tela | Só na demonstração: abrir uma tarefa, quantidade de cartões, cenário de erro |

### Estrutura

| Pasta | O que tem |
| --- | --- |
| `nucleo/` | Núcleo em Go: canal local, API `/v1`, dados, terminais e agentes, cópias isoladas, conversas do Claude Code, servidor MCP, navegador da tarefa e conexões de banco |
| `app/` | Tela em Rust + egui, com as fontes Inter e JetBrains Mono embutidas |
| `mascote/` | A abelha-robô, como biblioteca, e a vitrine em `examples/` |
| `docs/` | Arquitetura, decisões, dependências e imagens |
| `scripts/` | `medir.sh` (processador e memória) e `instalar.sh` |
| `packaging/` | Atalho (`.desktop`) e ícone |
| `arquivo/` | Os protótipos comparados na escolha da tecnologia da tela |

## Plataformas

| | Linux | macOS | Windows |
| --- | --- | --- | --- |
| Núcleo | sim | deve funcionar (não testado) | falta o canal por named pipe e os terminais por ConPTY |
| Tela | sim | deve funcionar (não testado) | compila; falta o canal |

No Linux, as senhas dos bancos vão para o chaveiro pelo Secret Service (o do GNOME, por exemplo); sem ele, ficam só na memória.

## Licença

Licença dupla, à sua escolha: [MIT](LICENSE-MIT) ou [Apache 2.0](LICENSE-APACHE). As fontes em `app/fontes/` seguem a SIL Open Font License 1.1.
