# 0002 · Canal local sem porta de rede

**Decidido em 29/09/2026.**

## Contexto

Os protótipos ligavam tela e núcleo por WebSocket em `127.0.0.1`, sem autenticação e aceitando qualquer origem. Como o núcleo digita nos terminais, qualquer página aberta no navegador poderia se conectar e executar comandos com o usuário.

## Decisão

- O núcleo escuta num socket Unix em `$XDG_RUNTIME_DIR/colmeia/` (ou no cache do usuário), com diretório `0700` e socket `0600`. Nenhuma porta TCP.
- Toda rota exige um token de 32 bytes, gerado a cada início, gravado em `token` (`0600`) e comparado em tempo constante. A tela lê o token do arquivo, nunca de variável de ambiente ou argumento.
- WebSocket com `Origin` de outro site é recusado.
- As cargas de teste só existem com `--demo`.
- No Windows, o canal será um named pipe com ACL só do usuário. Até existir, o núcleo recusa iniciar em vez de abrir uma porta.

## Consequências

- Nenhum outro usuário da máquina e nenhuma página web consegue falar com o núcleo.
- O acesso remoto (celular) vai precisar de um canal próprio, com pareamento, sem mudar esta regra para o uso local.
- A camada de "quem pede" já existe em toda rota, pronta para permissões por cliente.
