# Como contribuir

Obrigada pelo interesse! A Colmeia é um projeto pessoal, de código aberto, sem vínculo com nenhuma empresa. Contribuições são bem-vindas, desde que sigam os pontos abaixo.

## Rodar

Veja "Desenvolvimento e testes" no [README](README.md#desenvolvimento-e-testes). Para mexer na tela sem tocar nos seus dados de verdade, use pastas de teste:

```bash
export COLMEIA_DIR=/tmp/colmeia-teste/run COLMEIA_DADOS=/tmp/colmeia-teste/dados
export COLMEIA_CHAVEIRO_SERVICO=colmeia-teste   # senhas de banco num serviço de teste do chaveiro
go -C nucleo build -o ../bin/colmeia-nucleo ./cmd/colmeia-nucleo
COLMEIA_NUCLEO=$PWD/bin/colmeia-nucleo cargo run --release -p colmeia
# no fim: bin/colmeia-nucleo --encerrar (com as mesmas variáveis)
```

`COLMEIA_DEMO=1` abre a demonstração, com terminais de teste e cenários para a abelha.

## Antes de abrir um pull request

```bash
go -C nucleo test -race ./...
test -z "$(gofmt -l nucleo)"
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
```

O CI roda os mesmos passos, mais `govulncheck`, `cargo audit` e o gitleaks no histórico inteiro. Os testes com MySQL e PostgreSQL de verdade (`-tags integracao`) estão em "Desenvolvimento e testes" no [README](README.md#desenvolvimento-e-testes).

## Dependências

Uma dependência nova precisa de licença compatível com MIT e Apache-2.0 (nada GPL, LGPL ou AGPL) e entra na lista em [docs/dependencias.md](docs/dependencias.md).

## Estilo

- **Português** no código, nos comentários, nos nomes e nas mensagens, como no resto do projeto.
- Comentários curtos que explicam o **porquê**, não o quê.
- Mensagens para quem usa: claras e práticas ("Não consegui mover a tarefa: …"), com o motivo que o núcleo devolve.
- **Regra de negócio fica no núcleo.** A tela só mostra e pede.
- **Desempenho:** tudo por evento, nada de consulta periódica; a tela parada não redesenha; só o terminal em foco é tempo real.
- **Segurança:** nenhuma porta TCP, nada por shell, o núcleo valida tudo que vem da tela, dados com permissão 0700/0600, JSON estrito. Veja o [SECURITY.md](SECURITY.md).
- No núcleo, cada mudança nova tem teste.

## Dados de exemplo

Nada de dados reais em código, testes, exemplos, capturas ou issues: nem nomes de empresas, clientes ou produtos, nem conteúdo de terminal com caminhos ou segredos seus. Use exemplos genéricos como `loja-web`, `api-pedidos` e `cliente-x`.

## Decisões

Mudanças de arquitetura viram um registro curto em [docs/decisoes](docs/decisoes/), com contexto, decisão, alternativas e consequências.
