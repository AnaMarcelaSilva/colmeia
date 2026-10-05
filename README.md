# Colmeia

Eu comecei a usar vários agentes de IA ao mesmo tempo, um escrevendo testes, outro mexendo numa tela, outro investigando um bug, e logo o gargalo virei eu. Era terminal demais, contexto copiado de um lado para o outro e nenhuma ideia de quem estava esperando por mim. A Colmeia nasceu disso.

É um app desktop que junta tudo num lugar só. Cada projeto tem um quadro de tarefas, cada tarefa tem os seus agentes (Claude Code, Codex, Gemini CLI, OpenCode) com o terminal ali dentro, e uma abelha no canto avisa quando alguém precisa de você. No fim do dia, o que aconteceu já está registrado, pronto para a daily e para a sprint.

![O quadro de um projeto: um agente escrevendo testes, outro esperando a sua resposta e as tarefas em revisão e concluídas](docs/imagens/quadro.png)

Roda no Linux, é de código aberto e gratuito.

![A abelha-robô da Colmeia nos cinco estados: dormindo, trabalhando, aguardando você, bugado e comemorando](docs/imagens/abelha.gif)

## Instalação

Cada versão publicada tem os binários para Linux x86_64 na página de releases, com o `SHA256SUMS` ao lado.

```bash
sha256sum -c SHA256SUMS
mkdir colmeia && tar -xzf colmeia-linux-x86_64.tar.gz -C colmeia
./colmeia/instalar.sh   # instala em ~/.local/bin, com atalho e ícone, sem sudo
```

Se preferir compilar, os passos estão em [Desenvolvimento](#desenvolvimento).

Vale saber desde já que a Colmeia tem duas partes. A janela é só a tela. Quem cuida dos terminais é o núcleo, um processo em segundo plano que a janela inicia sozinha. Por isso fechar a janela não para os agentes, e eles seguem trabalhando (e gastando a sua conta de IA). Se houver agente rodando, a Colmeia pergunta se é para fechar só a janela ou parar tudo. Para encerrar o núcleo de vez, use `colmeia-nucleo --encerrar`.

Os seus dados ficam em `~/.local/share/colmeia/`, acessíveis só pelo seu usuário.

## Primeiros passos

Na primeira vez a Colmeia pede um perfil. Eu uso um para o trabalho e outro para estudo, cada um com os seus projetos, as suas contas de IA e o seu tema (escuro, claro ou leitura). Ela mostra as ferramentas de IA instaladas, e cada uma pode usar a conta do sistema ou uma conta só daquele perfil.

Depois é só adicionar um projeto. Pode ser um repositório git ou qualquer pasta, e adicionar não muda nada nela.

Para falar com um agente, crie uma tarefa em "+ Nova tarefa", abra a tarefa e clique em **Adicionar agente**. O terminal do Claude Code abre ali mesmo. Você pode digitar direto nele ou usar a caixa de mensagem embaixo, que envia com Ctrl+Enter, aceita imagens coladas e lembra das mensagens anteriores com a seta para cima.

Arrastar move o cartão entre colunas, e o botão direito (ou o "⋯") mostra o resto das ações.

## O que dá para fazer

### Quadro e agentes

Numa tarefa de repositório git, o agente pode trabalhar direto na pasta do projeto ou numa cópia isolada, um git worktree na branch da tarefa. Assim dois agentes não pisam um no outro.

Uma tarefa pode ter vários agentes. O que está em foco aparece em tempo real e os outros viram miniaturas. Se você começou uma conversa com o Claude Code no IntelliJ ou num terminal, **Retomar conversa** lista as que ele guardou para aquela pasta e continua de onde parou.

O cartão mostra o que cada agente está fazendo, com a mesma cor e o mesmo texto no quadro, no painel e na abelha.

| Estado | Como aparece |
| --- | --- |
| Trabalhando | ponto verde e a última linha do terminal |
| Pede aprovação | "Parece pedir aprovação · desde 14:32" |
| Sua vez | "Sua vez · desde 14:32" |
| Quer consultar o banco | um pedido esperando a sua aprovação |
| Parado | "Parado desde 14:10" |
| Terminou ou deu erro | "Terminou às 14:40", "Parou com erro (código 1) às 14:40" |

O "sua vez" é uma leitura, não uma certeza. Depois de alguns segundos sem saída, a Colmeia entende que o agente terminou e espera você, e por isso a tela diz "parece" ([decisão 0005](docs/decisoes/0005-estado-do-agente-por-heuristica.md)). O cartão anda sozinho entre "Agente trabalhando" e "Aguardando você", mas nunca vai para Revisão ou Concluído sem você mandar.

Quando algo precisa de você em outra tarefa, aparece um aviso no rodapé. Com a janela em segundo plano, o título vira "Colmeia · 2 esperando você". Ctrl+Shift+P leva direto ao próximo agente que está esperando.

### Linha do tempo, daily e sprint

![A apresentação da daily: o slide de uma tarefa com o que foi feito, os números, a nota e as fotos](docs/imagens/apresentacao.png)

Tudo o que acontece nas tarefas vira um registro. A **linha do tempo** mostra um dia por vez, com um cartão por tarefa e as capturas em miniatura. A **daily** junta o que mudou desde o último dia com atividade, separado em concluídas, em revisão, esperando você, trabalhando e com erro, e "Copiar texto" entrega o resumo pronto para falar. A **sprint** cobre 7 ou 14 dias, o mês ou as datas que você escolher, e exporta em Markdown com as capturas.

O **modo apresentação** (F5) foi pensado para a reunião. Ele passa a daily ou a sprint em tela cheia, com uma capa e um slide por tarefa. Cada slide tem a sua nota, que você escreve com N e que funciona como roteiro da fala, e aceita fotos e vídeos com A ou arrastando o arquivo para a janela. Fotos JPEG perdem o EXIF no caminho. Enquanto você apresenta, nenhum aviso aparece na tela.

![A linha do tempo: o cabeçalho do dia com os números e um cartão por tarefa, com as miniaturas](docs/imagens/linha-do-tempo.png)

### Pedir ao agente, navegador e arquivos

Da daily, da sprint ou do slide (P), dá para pedir algo a mais ao agente de uma tarefa sem abrir o terminal, como "traga o total de testes e complemente a nota" ou "capture prints das telas prontas". O pedido espera o agente terminar o que está fazendo e a resposta volta para a nota.

Isso funciona porque todo Claude Code que a Colmeia abre recebe ferramentas próprias (via MCP), presas à tarefa dele. Com elas o agente lê a tarefa e a nota, complementa a nota, anexa imagens, usa o navegador da tarefa, escreve na lousa e pede consultas ao banco. Codex, Gemini e OpenCode ainda não recebem essas ferramentas.

O **navegador da tarefa** é um Chrome com perfil próprio, nunca o seu, que abre ao lado da janela e pode ser capturado com Ctrl+Shift+B. A gaveta de **arquivos** (Ctrl+Shift+E) mostra a pasta da tarefa só para leitura, com pré-visualização de texto e imagem, e pede confirmação antes de mostrar `.env` e chaves.

### Lousa

![A lousa do workspace: notas em markdown, um trecho de terminal, uma tabela, um cartão de tarefa e as ligações tracejadas entre eles](docs/imagens/lousa.png)

Cada workspace tem uma lousa, e cada tarefa também (Ctrl+Shift+Q). É um quadro livre para notas em markdown, trechos de código, imagens, vídeos e cartões de tarefa ao vivo, ligados por setas.

Clique duplo no vazio cria uma nota, e N, T, C, I e K criam outros itens onde está o mouse. Para ligar dois itens, puxe a bolinha que aparece na borda de um até o outro, ou selecione os dois com Shift e use o botão de ligação. O agente também escreve na lousa da tarefa, e o que ele acrescenta aparece na hora, marcado como novo. Com F5, a lousa vira apresentação, seguindo as setas.

### Banco de dados

![A tela de bancos: a árvore com a conexão loja-web-dev aberta até as colunas da tabela pedidos e o console com um SELECT e a grade de resultado](docs/imagens/banco.png)

Cada perfil guarda as suas conexões de PostgreSQL, MySQL/MariaDB, SQLite e SQL Server (este ainda experimental), em Ctrl+Shift+K. A árvore vai da conexão às colunas, carregando aos poucos, e o console executa a instrução sob o cursor com Ctrl+Enter.

A senha fica no chaveiro do sistema. Ela nunca vai para o banco da Colmeia nem para os logs. Sem chaveiro, a senha é pedida ao conectar e fica só na memória.

Toda conexão começa somente leitura. Para alterar dados é preciso ligar "Permitir alterações", e mesmo assim cada alteração pede confirmação mostrando a instrução inteira. O agente pode pedir consultas de leitura numa conexão em que você liberou isso, mas nada roda até você aprovar na tela. A linha do tempo registra que houve consulta, nunca o SQL nem o resultado ([decisão 0009](docs/decisoes/0009-bancos-chaveiro-e-somente-leitura.md)).

## Atalhos

A Colmeia usa **Ctrl+Shift+letra** para ela, e esses atalhos não chegam ao terminal.

| Atalho | Ação |
| --- | --- |
| Ctrl+Shift+P | Próximo agente que precisa de você |
| Ctrl+Shift+L / Ctrl+Shift+D | Linha do tempo / Daily |
| Ctrl+Shift+K | Bancos de dados |
| Ctrl+Shift+S / Ctrl+Shift+B | Captura o terminal / o navegador da tarefa |
| Ctrl+Shift+E / Ctrl+Shift+Q | Arquivos / lousa da tarefa |
| Ctrl+Esc | Volta ao quadro |
| Ctrl+Enter, ↑ ↓ | Na caixa de mensagem, envia e navega pelas mensagens anteriores |
| Ctrl+Enter, Esc | No console de banco, executa e cancela |
| F5 / Shift+F5 | Apresenta / retoma do último slide |

Na apresentação, as setas, PgUp/PgDn e Espaço passam os slides, N edita a nota, A adiciona foto ou vídeo, P pede ao agente, L alterna entre anexos e lousa, H esconde o slide e ? mostra o resto.

## Por dentro

```
┌─────────────────────────────┐        ┌────────────────────────────┐
│ app/ (Rust + egui)          │        │ nucleo/ (Go)               │
│ a tela: só mostra e pede    │ ─────▶ │ estado, terminais, agentes │
│                             │  /v1   │ eventos e bancos           │
└─────────────────────────────┘        └────────────────────────────┘
        socket Unix (diretório 0700) + token
```

O núcleo, em Go, guarda tudo num SQLite com um histórico de eventos encadeado por hash, cuida dos terminais e valida cada pedido. A tela, em Rust com egui, só mostra e pede. As mudanças chegam por WebSocket, então a tela parada não consulta nada nem redesenha à toa, e só o terminal em foco roda em tempo real. O mesmo núcleo serve as ferramentas MCP dos agentes e controla o navegador por pipe.

A Colmeia não abre nenhuma porta de rede. O núcleo escuta num socket Unix dentro de uma pasta só sua, com um token novo a cada início, e cada agente tem um token que só vale para a tarefa dele. Nada passa por shell, os dados ficam com permissão só do usuário, e mensagens, notas e itens da lousa que parecem conter senha ou chave não são guardados.

Para quem quiser ir mais fundo, há a [arquitetura](docs/arquitetura.md), as [decisões registradas](docs/decisoes/), as [dependências](docs/dependencias.md) e o [modelo de segurança](SECURITY.md).

## Desenvolvimento

Você precisa de Go 1.26+, Rust 1.88+ e das bibliotecas de sistema do `eframe` (no Ubuntu, `libxkbcommon-dev`, `libgtk-3-dev` e os drivers de vídeo).

```bash
# Núcleo
go -C nucleo build -o ../bin/colmeia-nucleo ./cmd/colmeia-nucleo

# Tela (inicia o núcleo sozinha)
COLMEIA_NUCLEO=$PWD/bin/colmeia-nucleo cargo run --release -p colmeia

# Modo demonstração, com cargas de teste e cenários para a abelha
COLMEIA_DEMO=1 COLMEIA_NUCLEO=$PWD/bin/colmeia-nucleo cargo run --release -p colmeia

# Instalar o que foi compilado
./scripts/instalar.sh
```

Os testes são os mesmos que o CI roda, que ainda passa `govulncheck`, `cargo audit` e o gitleaks.

```bash
go -C nucleo test -race ./...
go -C nucleo vet ./... && gofmt -l nucleo
cargo test --workspace
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings
```

Os testes de integração com MySQL e PostgreSQL de verdade usam contêineres descartáveis.

```bash
docker run -d --name colmeia-teste-mysql -p 127.0.0.1:33061:3306 -e MYSQL_ROOT_PASSWORD=teste-raiz -e MYSQL_DATABASE=loja mysql:8
docker run -d --name colmeia-teste-pg -p 127.0.0.1:54321:5432 -e POSTGRES_PASSWORD=teste-raiz -e POSTGRES_DB=loja postgres:16-alpine
COLMEIA_TESTE_MYSQL=127.0.0.1:33061 COLMEIA_TESTE_PG=127.0.0.1:54321 go -C nucleo test -tags integracao ./internal/bancos/
docker rm -f colmeia-teste-mysql colmeia-teste-pg
```

Para testar sem tocar nos seus dados de verdade, aponte `COLMEIA_DIR` e `COLMEIA_DADOS` para pastas de teste, como explica o [CONTRIBUTING.md](CONTRIBUTING.md). O que mudou em cada versão está no [CHANGELOG.md](CHANGELOG.md).

<details>
<summary>Variáveis de ambiente</summary>

| Variável | Para quê |
| --- | --- |
| `COLMEIA_DIR` | Pasta do socket e do token (padrão `$XDG_RUNTIME_DIR/colmeia`) |
| `COLMEIA_DADOS` | Pasta dos dados (padrão `~/.local/share/colmeia`) |
| `COLMEIA_NUCLEO` | Caminho do `colmeia-nucleo`, se ele não estiver ao lado da tela nem no PATH |
| `COLMEIA_DEMO=1` | Inicia o núcleo em modo demonstração |
| `COLMEIA_EDITOR` | Editor usado em "Abrir no…" |
| `COLMEIA_NAVEGADOR` | Chrome ou Chromium do navegador da tarefa |
| `COLMEIA_CHAVEIRO=memoria` | Ignora o chaveiro e guarda as senhas só na memória |
| `COLMEIA_CHAVEIRO_SERVICO` | Nome do serviço no chaveiro (padrão `Colmeia`) |
| `COLMEIA_TEMA` | `escuro`, `claro` ou `leitura`, antes de escolher um perfil |
| `COLMEIA_SEM_ABELHA=1` | Desliga a abelha |
| `COLMEIA_TAMANHO` | Tamanho inicial da janela, como `1280x720` |
| `COLMEIA_FPS=1` | Mostra o contador de quadros, para medir desempenho |

</details>

O código está dividido em `nucleo/` (Go), `app/` (a tela), `mascote/` (a abelha, com uma vitrine em `cargo run --release -p mascote --example vitrine`), `docs/`, `scripts/` e `packaging/`. Em `arquivo/` ficaram os protótipos que comparei antes de escolher Rust e egui para a tela.

## Plataformas

O dia a dia é no Linux. No macOS deve funcionar, mas nunca testei. No Windows a tela compila, mas o núcleo ainda precisa do canal por named pipe e dos terminais por ConPTY.

## Licença

À sua escolha, [MIT](LICENSE-MIT) ou [Apache 2.0](LICENSE-APACHE). As fontes em `app/fontes/` seguem a SIL Open Font License 1.1.
