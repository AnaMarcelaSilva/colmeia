# Segurança

A Colmeia controla terminais onde agentes de IA rodam comandos. Quem conseguir falar com o núcleo consegue executar comandos com o usuário de quem o iniciou. Por isso o canal entre a tela e o núcleo é a parte mais protegida do projeto.

## Modelo de ameaça

| Quem | O que poderia tentar | Proteção |
| --- | --- | --- |
| Página aberta no navegador | Abrir uma conexão para `127.0.0.1` e digitar nos terminais | O núcleo não escuta em nenhuma porta TCP; um WebSocket com `Origin` de outro site é recusado |
| Outro usuário da máquina | Conectar no socket ou ler o token | Diretório do canal `0700` e do dono, socket e token `0600`; o núcleo recusa um diretório de outro usuário |
| Outro processo do mesmo usuário sem o token | Usar a API | Toda rota exige `Authorization: Bearer <token>`, comparado em tempo constante |
| Um segundo núcleo | Tomar o canal de um que está rodando | O núcleo só remove um socket antigo se ninguém responde nele |
| Uma tela com defeito ou maliciosa | Esgotar memória ou travar o núcleo | Mensagens de até 1 MB, tamanho de terminal limitado, ritmo de envio entre 8 ms e 5 s, controle de fluxo |
| Um pedido com dados maliciosos | Injetar comandos no git ou SQL | O git roda sem shell, com os argumentos direto no executável e sem pedir senha; nomes de branch que pareçam opção (`--algo`) são recusados; o SQL usa só parâmetros; o JSON recusa campos desconhecidos e corpos acima de 64 KB |
| Outro usuário da máquina | Ler projetos, tarefas ou imagens coladas | Dados em `~/.local/share/colmeia` (0700), banco e imagens coladas com permissão 0600 |
| Um pedido para abrir agente | Rodar outro programa ou passar opções à ferramenta | A ferramenta é uma de uma lista fixa (ou o shell do usuário), achada no PATH; o id de conversa só passa se tiver a forma de um UUID; nada vai por shell |
| Um agente que não termina | Deixar processos rodando depois de removido | Cada agente tem sessão e grupo de processos próprios; ao remover, o grupo recebe SIGHUP e, depois de 3 segundos, SIGKILL |
| Remover uma tarefa com cópia isolada | Perder trabalho que ainda não está num commit | A remoção é recusada se a cópia tiver mudanças fora de um commit, e os agentes continuam rodando |

Para avisar que o Claude Code está aberto na pasta fora da Colmeia, o núcleo lê em `/proc` a pasta e o nome dos processos do próprio usuário (os de outros usuários não são acessíveis); os agentes da Colmeia são reconhecidos pela variável `COLMEIA_AGENTE`. As conversas do Claude Code são só lidas, nunca alteradas.

O histórico de mudanças é encadeado por hash: o núcleo refaz a corrente ao iniciar e avisa se algum evento foi alterado por fora.

O token é gerado a cada início (32 bytes aleatórios), gravado de forma atômica e lido do arquivo pela tela. Ele nunca passa por variável de ambiente ou argumento de linha de comando, que outros processos conseguem ver. O conteúdo dos terminais nunca vai para o log.

## O que ainda não está protegido

- **Aprovações.** As ações arriscadas (push, merge, deploy, escrita em banco) ainda não passam por aprovação; isso chega com o núcleo como servidor MCP para os agentes.
- **Isolamento dos agentes.** Os agentes rodam com as permissões do usuário, como num terminal comum.
- **Windows.** O canal por named pipe (com ACL só do usuário) ainda não existe; até lá, o núcleo recusa iniciar no Windows em vez de abrir uma porta de rede.

## Relatar uma vulnerabilidade

Use o recurso de relato privado de vulnerabilidades do GitHub no repositório (aba *Security*), sem abrir uma issue pública.
