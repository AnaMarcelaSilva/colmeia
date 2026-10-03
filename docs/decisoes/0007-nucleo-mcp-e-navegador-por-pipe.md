# 0007 · O núcleo como servidor MCP dos agentes e o navegador controlado pelo pipe

**Decidido em 02/10/2026.**

## Contexto

Na daily, o usuário quer pedir algo a mais ao agente de uma tarefa ("traga o total de testes e complemente a nota", "capture prints das telas prontas") e ver a resposta na nota e no slide sem sair dali. Para isso o agente precisa de um jeito de escrever na nota, anexar imagens e abrir e capturar a tela que criou, sem ganhar acesso ao resto da Colmeia.

## Decisão

- **O próprio núcleo é o servidor MCP**, num subcomando (`colmeia-nucleo mcp`) que o Claude Code roda por stdio. Ele fala com o núcleo pelo socket local, com um token só do agente. As rotas dos agentes (`/v1/agente/*`) não levam id: a tarefa sai do token. O token da tela não entra nelas e o do agente não entra em nenhuma outra.
- **O token do agente fica só em memória** (pelo sha256) e num arquivo `0600` no diretório do canal, que é tmpfs. Não vai em argumento nem em variável de ambiente, e é revogado quando o agente termina. A configuração MCP é um arquivo `0600` por agente, passado por `--mcp-config`.
- **Ferramentas aprovadas de antemão** (`--allowedTools mcp__colmeia`): tudo nelas já está confinado à tarefa. Sem `--strict-mcp-config`, os outros servidores MCP do usuário continuam valendo.
- **O pedido entra no terminal por evento**, na passagem para `esperando resposta`, nunca durante um pedido de aprovação (o texto responderia a pergunta) e nunca com digitação nos últimos 5 s. Ele vai como colagem, como a tela manda as mensagens.
- **O navegador é um Chrome ou Chromium com `--remote-debugging-pipe`**, nunca uma porta: uma porta de depuração deixaria qualquer processo da máquina controlar o navegador. O protocolo (CDP) é escrito com `encoding/json`, sem dependência nova. A pasta de perfil é da Colmeia (`0700`), um processo por perfil e uma janela por tarefa. Só `http`, `https` e `file://` de dentro da pasta; nos eventos, o endereço vai sem a query.
- **Os caminhos de arquivo passam por `os.Root`** (anexar uma imagem da pasta, a árvore e a pré-visualização): caminho absoluto de fora, `..` e link simbólico para fora são recusados pelo próprio Go; `.git`, `node_modules` e `target` não abrem.

## Alternativas

- **Um servidor MCP separado (outro binário ou Node):** mais uma peça para instalar e versionar. O subcomando usa o mesmo executável e o mesmo canal.
- **Token em variável de ambiente:** aparece em `/proc/<pid>/environ` de qualquer filho do agente e nos dumps. O arquivo `0600` no tmpfs é lido uma vez pelo processo MCP.
- **Porta de depuração do Chrome (`--remote-debugging-port`):** qualquer processo local (e páginas, por DNS rebinding) poderia controlar o navegador. Recusado, como a porta TCP do núcleo ([decisão 0002](0002-canal-local-sem-rede.md)).
- **Navegador embutido na janela egui:** traria um motor web para dentro da tela. Fora do objetivo.
- **Escrever o pedido no terminal na hora:** juntaria o texto a uma resposta de aprovação ou ao que o usuário digitava. A fila por estado evita isso sem consulta periódica.

## Consequências

- Só o Claude Code ganha o MCP por enquanto; Codex, Gemini e OpenCode configuram de outro jeito.
- Um agente que já rodava antes desta versão só ganha as ferramentas quando for iniciado de novo.
- Se o usuário tiver texto pela metade na caixa do Claude Code, o pedido ainda pode se juntar a ele (passados os 5 s). O evento `pedido.entregue` aparece na tela.
- Uma página aberta no navegador pode tentar instruir o agente (o mesmo risco do WebFetch); as ferramentas da Colmeia só agem dentro da tarefa.
- No Wayland o compositor pode ignorar a posição da janela. Durante a apresentação, a janela aberta pelo agente vai para fora da área visível; um gerenciador de janelas que force janelas novas para dentro da tela pode trazê-la para frente.
