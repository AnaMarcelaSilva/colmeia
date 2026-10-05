# 0011 · Windows com socket Unix, ConPTY e job objects

**Decidido em 05/10/2026.**

## Contexto

A Colmeia nasceu para Linux e Windows, mas até a 0.3.0 o núcleo só rodava no Linux e no macOS. Faltavam o canal entre a tela e o núcleo, os terminais dos agentes e o pipe de controle do navegador. A decisão 0002 previa um named pipe com ACL só do usuário.

## Decisão

- **Canal: socket Unix (AF_UNIX), não named pipe.** O Windows 10 (1803) e o 11 têm sockets Unix no Winsock. O Go escuta neles com o mesmo `net.Listen("unix")`, e a tela usa o `uds_windows` (MIT), que imita o `UnixStream` do Rust. Assim o protocolo, o token e o código são os mesmos nos dois sistemas, sem porta de rede. O diretório (`%LOCALAPPDATA%\Colmeia\canal`) recebe uma lista de acesso protegida, que não herda a de cima, só com o usuário e o sistema. O socket, o token e as configurações dos agentes herdam essa lista. Os dados (`%LOCALAPPDATA%\Colmeia\dados`) recebem a mesma proteção.
- **Terminais: ConPTY.** O programa do agente roda num pseudoconsole, e o núcleo lê e escreve VT pelos pipes, como no Linux. Ele nasce suspenso, entra num job object com "encerrar ao fechar" e só então começa. Fechar o terminal fecha o console (o programa recebe o aviso e pode salvar) e, depois de 3 s, encerra a árvore inteira pelo job, como o grupo de processos no Unix. Um `.cmd` (o `claude` do npm) roda pelo `cmd.exe`, e aí nenhum argumento pode ter caractere especial do cmd. Hoje os argumentos são só caminhos e identificadores.
- **Navegador: o mesmo `--remote-debugging-pipe`.** No Windows, a biblioteca C do Chrome recebe os descritores 3 e 4 pelo bloco `lpReserved2` do `CreateProcess`, o mesmo mecanismo que o Node usa. Só essas duas pontas são herdadas (lista de handles), e o navegador fica num job. Sem Chrome instalado, o Edge, que vem com o Windows, também serve.
- **Caminhos.** `file:///C:/...` vira `C:\...`, e a conferência de "dentro da pasta" usa o separador do sistema e recusa caminhos que começam na raiz da unidade ou em outra unidade.

## Alternativas

- **Named pipe:** precisaria de outro protocolo de conexão na tela (o Rust não tem pipe nomeado com tempo-limite e poll na biblioteca padrão) e de um caminho de código só para o Windows em todo lugar que hoje usa o socket.
- **Porta TCP em 127.0.0.1:** qualquer processo da máquina, inclusive uma página no navegador, poderia tentar falar com o núcleo. Fora de questão (decisão 0002).

## Consequências

- Windows anterior ao 10 1803 não roda.
- Os testes do Windows (canal, ConPTY, navegador e caminhos) rodam no CI do GitHub. A validação na tela fica com quem usa Windows.
- `processos.RodandoFora` (o Claude Code aberto fora da Colmeia na mesma pasta) ainda só funciona no Linux.
