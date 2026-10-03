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
| Uma página ou processo querendo ouvir o que acontece | Ler os eventos dos perfis | O WebSocket de eventos exige o mesmo token e recusa `Origin` de outro site; é só de leitura (o que a tela manda por ele é descartado), aceita mensagens de até 4 KB, no máximo 16 telas ao mesmo tempo, e cada tela só recebe o próprio perfil |
| Uma imagem maliciosa | Esgotar memória (bomba de descompressão) ou esconder dados no arquivo | Imagens só como PNG ou JPEG, até 8 MB e 8192×8192 (40 milhões de pixels), conferidas pelo cabeçalho antes de decodificar; a imagem é decodificada e gravada como um PNG novo (sai sem metadados: o EXIF e o GPS de uma foto ficam para trás), o nome do arquivo é o hash do conteúdo, nunca o que veio no pedido; arquivo `0600` numa pasta `0700` |
| Um vídeo malicioso | Encher o disco ou fazer o sistema abrir outra coisa com nome de vídeo | Até 512 MB, copiado em fluxo para um temporário `0600` que sai em qualquer erro; os primeiros bytes precisam bater com o tipo enviado (mp4, webm, mkv ou mov) e a extensão vem de uma lista fixa; a tela não decodifica vídeo, chama `xdg-open` com um argumento só e sem shell |
| Um segredo colado na caixa de mensagem ou numa nota | Ficar guardado no histórico de mensagens ou aparecer no telão | O núcleo não guarda o que parece senha, token ou chave (a nota é recusada com "Não salvei"); o histórico fica fora da corrente de eventos, com no máximo 200 mensagens por agente, e "Limpar histórico de mensagens" apaga tudo |
| Datas e filtros da linha do tempo | Pedir períodos enormes ou parâmetros estranhos | Datas no formato `AAAA-MM-DD` e até 92 dias, `limite` entre 1 e 500, projeto conferido como do perfil |
| Um agente usando as ferramentas da Colmeia (MCP) | Mexer em outra tarefa, ler arquivos de fora da pasta ou usar o resto da API | Cada agente tem um token próprio (32 bytes, guardado no núcleo só pelo sha256), num arquivo `0600` do diretório do canal, nunca em argumento ou variável de ambiente, revogado quando o agente termina. Esse token só entra nas rotas `/v1/agente/*`, que não levam id: a tarefa sai do token. Caminhos passam por `os.Root` (fora da pasta, `..` e link para fora são recusados); notas e legendas com cara de segredo são recusadas |
| Uma página no navegador da tarefa | Controlar o navegador, ler arquivos locais ou aparecer no compartilhamento | O Chrome é controlado só por `--remote-debugging-pipe` (nenhuma porta de depuração), com perfil próprio da Colmeia (`0700`). Só `http`, `https` e `file://` de dentro da pasta, sem `.env` e chaves, conferidos também em redirecionamentos e sub-recursos; a captura é recusada se a página saiu da pasta. Nos eventos, o endereço vai sem a query. Durante a apresentação, a Daily e a Sprint, a janela aberta pelo agente fica fora da tela |
| A árvore de arquivos da tarefa | Ler arquivos de fora da pasta ou mostrar um segredo sem querer | Só leitura, por `os.Root`, um nível por vez e com limite de itens e de tamanho; `.env`, chaves e certificados só aparecem depois de confirmar; "Abrir no sistema" só para documentos e imagens, sem shell |

Para avisar que o Claude Code está aberto na pasta fora da Colmeia, o núcleo lê em `/proc` a pasta e o nome dos processos do próprio usuário (os de outros usuários não são acessíveis); os agentes da Colmeia são reconhecidos pela variável `COLMEIA_AGENTE`. As conversas do Claude Code são só lidas, nunca alteradas.

O histórico de mudanças é encadeado por hash: o núcleo refaz a corrente ao iniciar e avisa (no log e na tela) se algum evento foi alterado por fora. O perfil, o projeto, a tarefa e o agente de cada evento ficam dentro dos dados cobertos pelo hash (`_escopo`) e também em colunas indexadas; a verificação confere que as colunas batem com o `_escopo`, então mudar um evento de perfil por fora também é detectado. Os eventos gravados antes dessas colunas foram ligados aos perfis uma única vez, ao atualizar o banco; essa ligação mexe só nas colunas, nunca nos dados nem no hash, e esses eventos antigos não têm `_escopo` para conferir.

**O conteúdo dos terminais nunca vai para os eventos.** O estado de um agente ("pede aprovação", "esperando resposta") é lido da saída do terminal dentro do núcleo, mas o que sai de lá é sempre um texto de uma lista fixa. Nada do que o agente escreveu entra no banco, no log ou nas mensagens para a tela.

**Capturas podem conter segredos.** Uma captura de terminal guarda tudo o que estava visível, inclusive senhas ou chaves. A Colmeia avisa na primeira captura de cada perfil; a imagem fica só no computador, em `~/.local/share/colmeia/anexos/` (`0600`), e sai de lá só quando você salva a sprint numa pasta escolhida. "Desfazer" (logo depois) e "Remover" (na linha do tempo) apagam o arquivo.

O token é gerado a cada início (32 bytes aleatórios), gravado de forma atômica e lido do arquivo pela tela. Ele nunca passa por variável de ambiente ou argumento de linha de comando, que outros processos conseguem ver. O conteúdo dos terminais nunca vai para o log.

## O que ainda não está protegido

- **Aprovações.** As ações arriscadas (push, merge, deploy, escrita em banco) ainda não passam por aprovação.
- **Isolamento dos agentes.** Os agentes rodam com as permissões do usuário, como num terminal comum. Por isso o token restrito do agente limita o que ele faz pelas ferramentas da Colmeia, mas não é uma barreira contra um agente decidido a burlá-lo: com o mesmo usuário, ele consegue ler o token da tela no diretório do canal.
- **Páginas que instruem o agente.** Uma página aberta no navegador da tarefa pode tentar dar ordens ao agente (o mesmo risco do WebFetch); as ferramentas da Colmeia só agem dentro da tarefa.
- **Windows.** O canal por named pipe (com ACL só do usuário) ainda não existe; até lá, o núcleo recusa iniciar no Windows em vez de abrir uma porta de rede.

## Versões com suporte

Só a versão mais recente recebe correções de segurança.

## Relatar uma vulnerabilidade

Use o recurso de relato privado de vulnerabilidades do GitHub no repositório (aba *Security*), sem abrir uma issue pública.
