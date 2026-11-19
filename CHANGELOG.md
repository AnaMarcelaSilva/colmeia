# Mudanças

O formato segue [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/) e as versões seguem [SemVer](https://semver.org/lang/pt-BR/). A Colmeia foi construída em entregas (A a H); dentro de cada versão, as mudanças vêm agrupadas pela entrega que as trouxe, da mais nova para a mais antiga.

## [Não lançado]

### Adicionado

- **A daily e a sprint como leitor, com a pauta na sua mão:** as duas páginas viraram a pauta à esquerda (por projeto na daily, por assunto na sprint) e a tarefa inteira à direita: o tópico, o estado, a frase para quem não é dev, os cartões técnicos da montagem (o que mudou, o endpoint, a coluna, como funciona, como testou, o que falta) e a galeria com as imagens passando com seta e a legenda. A alça arrasta para mudar a ordem, que fica guardada por reunião; o visto marca o que já foi apresentado e o alto da página conta "2 de 8 apresentadas". "Daily apresentada" fecha a daily de hoje: os vistos zeram e a próxima começa dali, com o que ficou aberto e sem o que já foi falado e concluiu. "Apresentar" (e o F5) abre o mesmo leitor em tela cheia, com a pauta compacta, a letra maior e as teclas (↑↓ tarefa, ←→ imagem, espaço apresentada, Enter abre a tarefa, Q a lousa da reunião, Esc sai); a apresentação em slides continua no "⋯" e no Shift+F5. Outras telas abertas acompanham pela pauta (`pauta.mudou`, `daily.apresentada`).
- **A reunião montada pela Colmeia:** cada tarefa ganha uma montagem para a reunião, um tópico de poucas palavras, uma frase para quem não é técnico e de um a seis cartões (o resumo, uma mudança por cartão, o fluxo numerado, as imagens com legenda, o que falta por último). A simples sai da nota, dos anexos e do que aconteceu, na hora e sem IA; o montador pede ao Claude Code, sem terminal, a montagem no estilo da reunião a cada mudança da tarefa (nota, anexo, coluna, o agente terminou a vez), depois de 3 minutos quietos, e a opção "Montar a reunião com IA" no "⋯" da daily desliga. As páginas Daily e Sprint mostram o tópico e a frase de cada tarefa, com o título e a origem embaixo, e o botão direito remonta, sem ou com IA. O agente da tarefa ganhou `montar_quadro_da_reuniao` para quando você pedir o quadro em dia.
- **A lousa da reunião:** a daily de hoje e a sprint têm uma lousa, aberta pelo botão "Lousa da reunião" na barra das páginas. A Colmeia desenha nela uma coluna por tarefa, na ordem da reunião: o cartão da tarefa em cima, os cartões descendo com setas e o último ligado à tarefa seguinte, com o nome do projeto (ou do assunto) acima de cada grupo. A lousa se redesenha quando uma montagem muda; o que você move ou edita vira seu e fica onde está. O palco (tela cheia) anda cartão por cartão, tarefa por tarefa.
- **A apresentação em papel, com a tabela da reunião:** a apresentação abre no tema Leitura (fundo claro, que projeta melhor; T troca para o escuro), o slide de cada projeto virou a tabela tópico | frase com um fio entre as linhas, como o slide que você monta à mão, e o slide da tarefa mostra os cartões da montagem, com a imagem escolhida por ela em primeiro. Q abre o palco da lousa da reunião.
- **O agente escreve como você:** as instruções da nota da daily, da proposta da sprint e do montador passaram a vir de um estilo só, escrito a partir das suas anotações: itens começando pelo substantivo, o sintoma e a causa de um erro, o que é novo com o nome técnico e a tecnologia (coluna nova, endpoint novo na API em Go, tela nova em React), o fluxo numerado de uma integração, o contexto do cliente antes e, por último, como testou, o que falta e de quem depende. Fica de fora só o que ninguém fala em reunião: arquivo, função, commit, branch, caminho, código e SQL. A nota de conclusão cresceu para 900 caracteres e 12 linhas.

- **A tarefa muda de projeto:** no menu do cartão (botão direito ou "⋯"), "Transferir para outro projeto…" leva a tarefa a outro quadro do perfil, com as notas, a lousa, os anexos, os pedidos e o histórico. A coluna fica; a branch só segue se existir no outro repositório (uma pasta sem git a apaga). Os agentes da tarefa são parados e removidos antes, porque trabalham na pasta de agora, e o diálogo avisa quantos são. Uma tarefa com cópia isolada não muda, porque a cópia é do repositório de antes. A linha do tempo conta "Levou a tarefa de X para Y", e a linha do tempo de uma tarefa mostra a história dela nos dois projetos. Para o agente, `transferir_tarefa` leva outra tarefa do projeto dele (não a própria, que ele está rodando) e `criar_tarefa` ganhou `projeto`: "cria no Estoque Central" cria lá, não no quadro em que ele está; `listar_projetos` mostra os nomes. Um nome que não existe volta com a lista.
- **Daily em blocos, como você fala:** a página Daily passou a ser um bloco por projeto, na ordem da barra lateral, com uma linha por tarefa dentro dele. Uma nota com itens aparece item a item, do jeito que foi escrita (lista numerada, pendência no fim), em vez de um parágrafo colado no título. O dia, quando não é hoje, e o que pede atenção viram selos na linha, e o alto da página conta as tarefas que esperam a sua resposta. "Copiar texto" entrega o texto montado das notas, em blocos, com "Esperando minha resposta" no fim. O campo de edição cresceu para cinco linhas.
- **Um slide por projeto na apresentação:** a daily e a sprint ganharam um slide por bloco (o projeto, ou o assunto da sprint, como título e as tarefas com as notas uma embaixo da outra), no lugar do divisor vazio seguido de um slide por tarefa. Oito tarefas em quatro projetos passam de 14 telas para 6. Um bloco que não cabe segue em partes, sem cortar tarefa. As tarefas com fotos, vídeos ou lousa continuam com o slide delas, logo depois do bloco; qualquer outra abre em detalhe com um clique ou Enter, e Esc volta ao bloco. No slide da tarefa a nota subiu para logo abaixo do título e "O que foi feito" foi para o fim.
- **O agente sabe como escrever a nota da daily:** o pedido que entra no terminal quando a tarefa vai para Revisão ou Concluído, a descrição de `complementar_nota` e as instruções do servidor MCP passaram a dizer o formato da nota (a primeira linha diz o que mudou para quem usa, um item por mudança quando houve mais de uma, o que falta por último, sem nomes de arquivo, função, commit, tabela ou SQL, até 600 caracteres, imagem só quando há uma tela para mostrar, nunca de terminal ou código). O texto vive num lugar só, `internal/formato`, com um exemplo.
- **O agente propõe o Play, você aceita:** o Claude Code da tarefa ganhou `propor_play` e `listar_plays`. Quando você pede "cria um play para subir a API", ele propõe a configuração de execução (nome, comando, pasta e variáveis), no projeto da tarefa ou em outro do mesmo workspace. A proposta aparece em "Propostas do agente" nas Configurações de execução, com o comando à vista, e só roda depois de "Aceitar" (dá para editar ou descartar). Comando e variáveis com cara de segredo são recusados: o valor fica para você preencher. A proposta e o aceite entram na linha do tempo, e a tela avisa quando chega uma.
- **O Play do workspace inteiro:** o menu do botão Rodar traz, embaixo das configurações do projeto, as dos outros projetos do workspace, cada uma na pasta do projeto dela. Dá para subir a API e o app de dentro de uma tarefa só; a saída aparece no mesmo painel.

### Mudado

- **As datas da sprint são suas:** mudar o começo ou o fim de uma sprint deixou de ser recusado quando o período cruza o de outra ("o período cruza a sprint de 09/11 a 13/11"). A sprint é um recorte de tarefas num período, e cruzar a vizinha não atrapalha: no dia que duas têm, a atual é a que começou por último. Só repetir exatamente as datas de outra continua recusado, porque as notas da sprint são guardadas pelo período. O campo de data também aceita o que você digita com folga: "1/9/2026", "12//11/2026" ou "12.11.2026" valem.
- **"O agente parou. E a tarefa?":** a caixa que aparece ao parar e remover um agente deixou de ser uma fileira de colunas coloridas e virou uma escolha explicada, uma opção por linha com a consequência embaixo (deixar onde está, que vem marcada; enviar para revisão; concluir; voltar ao backlog) e um Confirmar só (Enter). "Agente trabalhando" saiu, porque sem agente a tarefa não vai para lá. "Excluir tarefa…" ficou como um link vermelho, longe dos botões, e continua pedindo confirmação.

### Corrigido

- **Terminal embaralhado "do nada":** um agente aberto ou retomado pela abelha fixa nascia num terminal de 100 colunas por 30 linhas, e a Colmeia, que só manda o tamanho dela quando o dela muda, mostrava esse desenho numa grade de 180 por 36: linhas de duas larguras misturadas, a linha de estado do Claude Code no meio da tela e restos nas colunas da direita. A abelha deixou de informar tamanho, o núcleo abre um agente sem tamanho no da última tela da Colmeia, e a tela manda o tamanho de novo toda vez que o terminal ganha o foco.
- **O agente não via a descrição da tarefa:** `ler_tarefa` só trazia a nota da daily de hoje e a última de sprint. Uma tarefa anotada ontem parecia vazia ("a tarefa não tem descrição além do título"), enquanto a aba Descrição mostrava a nota. Agora a ferramenta traz também a descrição, que é a nota mais recente de qualquer dia, com quem a escreveu.
- **Ctrl+Z emudecia o agente:** o Ctrl+Z no terminal suspendia o Claude Code (ele se manda o sinal de parar), e sem um shell por trás nada o trazia de volta: ficava só um "^Z" na tela até remover o agente. A tela não manda mais o Ctrl+Z e o núcleo também o descarta do que chega por qualquer caminho.
- **Cartão do quadro sem cortar o título nem vazar a branch:** o título vai em até duas linhas, quebrando nas palavras, antes das reticências, e o cartão cresce junto. Uma branch sem tipo conhecido (que ia inteira numa etiqueta âmbar) agora é cortada com reticências dentro do cartão. O prefixo `tarefa/`, o que a própria Colmeia sugere ao criar a tarefa, virou um tipo com etiqueta, como `feature/` e `fix/`.
- **Rotinas que ficavam no erro:** quando o núcleo demorava mais de 10 segundos para responder, a tela de Rotinas mostrava "Resource temporarily unavailable" e ficava assim até trocar de workspace. Agora ela tenta de novo sozinha (três vezes, com 2, 4 e 6 segundos de espera) e, se ainda falhar, oferece "Tentar de novo". E o núcleo passou a registrar em `nucleo.log`, na pasta de dados, o que escreve no stderr e cada pedido que passa de 3 segundos, para a próxima demora ter rastro.
- **A abelha fixa lê o Claude Code atual:** o Claude Code 2.1 desenha a tela por posição (vá para a linha tal, coluna tal) em vez de escrever linha a linha, e o núcleo, que só tirava as cores, entregava a tela inteira numa linha só, sem espaços. O núcleo agora mantém uma grade de terminal por agente (cursor, apagar, inserir linhas, região de rolagem, tela alternativa, caracteres largos) e entrega as linhas como estão na tela. A conversa da abelha também entende o formato novo: a sua mensagem com "❯", o resumo das ferramentas debaixo da frase ("Leu 1 arquivo", "Rodou 3 comandos"), o menu de pergunta com a descrição de cada opção, e deixa de fora o logo, os traços, a caixa de digitar e a linha de estado.
- **Enter que virava quebra de linha:** uma mensagem enviada ao agente pela abelha, pela caixa da tarefa ou por um pedido do núcleo às vezes ficava na caixa do Claude Code sem ir, porque a rajada de texto com o Enter colado atrás era tomada por colagem. Agora o texto vai entre as marcas de colagem e o Enter um instante depois.
- **Concluir pelo agente também encerra o chat:** quando o agente levava a tarefa ao Concluído a seu pedido, ele continuava ativo. Agora, depois de responder a frase da reunião, os agentes da tarefa saem como quando você conclui pela tela.
- **O teclado vai para o terminal ao abrir a tarefa:** a tela abria com o foco na caixa de mensagem de baixo; agora o terminal do agente em foco recebe o teclado.
- **Enter salva a frase da reunião depois de um acento:** na daily e na sprint, o Enter às vezes não saía do campo da frase. O campo esperava o fim de um acento que o teclado já tinha encerrado sem avisar (o ibus no X11 faz isso). Agora qualquer tecla ou texto digitado encerra a espera.
- **Tarefa removida sai da daily:** "Tirar desta daily" numa tarefa já removida dava "não encontrado". Agora ela sai e volta como as outras, e aparece na lista de tiradas com o título que tinha.
- **Abelha fixa ao sair da Colmeia por Alt+Tab:** a abelha só voltava ao minimizar ou fechar a janela. Agora ela aparece quando a janela da Colmeia deixa de ser a que está em uso, e some de novo quando você volta. No X11 a janela escuta qual é a janela ativa; fora dele vale o foco que o egui informa.
- **Concluído de hoje pelo dia da tarefa:** uma tarefa concluída hoje não aparecia no Concluído do quadro quando o agente dela tinha terminado em outro dia; a coluna olhava a data do agente. Agora a tarefa guarda quando entrou no Concluído e o quadro usa esse dia. As já concluídas ganham a data pelo histórico ao abrir.
- **Concluir tira os agentes:** levar a tarefa ao Concluído remove os agentes dela, como o Remover de cada um. O que eles fizeram continua na linha do tempo.
- **Pergunta livre com IA guardada:** a opção que usa tokens saiu do menu e dos ajustes desta versão, e a pergunta não é mandada mesmo num perfil que a tinha ligado. Continua no núcleo, para voltar quando for a hora.

### Novo

- **O agente cria e move tarefas a seu pedido:** `criar_tarefa` cria uma tarefa no projeto dele (no Backlog ou na coluna pedida, com a explicação na descrição), `listar_tarefas` mostra as do projeto e `mover_tarefa` leva a sua ou outra tarefa do projeto para qualquer coluna do quadro, inclusive direto ao Concluído quando você disser que já terminou. As descrições das ferramentas pedem que só aconteça quando você mandar na conversa.
- **Rotina desde a última tag:** o período "Desde a última tag" pega os commits de cada projeto desde a última tag dele (a versão lançada) e o quadro desde a tag mais antiga entre os projetos; sem tag, os últimos 30 dias. Serve para montar o changelog da versão quando você mandar, sem data fixa. E os projetos cadastrados como pasta que são um repositório git passaram a entrar nos commits das rotinas.
- **Permissão de banco por conexão para os agentes:** na conexão, além de "Agentes podem consultar", dá para marcar "Sem pedir aprovação" (as consultas rodam na hora, até você desmarcar) e "Também alterar dados" (INSERT, UPDATE e DELETE com WHERE, só numa conexão que já permite alterações; DDL e instruções sem WHERE continuam recusadas). A árvore de bancos mostra o selo em âmbar quando uma das duas está ligada, e o histórico da conexão registra cada instrução.
- **Rotinas do workspace:** na barra lateral, junto de Lousa e Conhecimento. Uma rotina é um pedido guardado, como "liste o que entrou na semana, por projeto, em Corrigido, Melhorado e Novo", que o Claude Code responde em texto a partir do quadro do período (tarefas, frases da daily e da sprint) e dos commits dos projetos. Roda quando você manda ou no horário da agenda (dias da semana e hora, com a Colmeia aberta). A tela é uma tabela com a última e a próxima rodada e uma gaveta com o texto, para copiar, e as rodadas anteriores. Roda sem ferramentas, numa pasta vazia, com a conta do perfil.
- **Enviar a branch e abrir o PR pela aba Arquivos:** no "Mudou", uma faixa com a branch da tarefa diz quantos commits ainda não foram ao origin, e "Enviar e abrir o PR" faz o git push (com as credenciais do seu git, sem perguntar senha) e abre a página do GitHub para criar o PR. Enviada, "Abrir o PR" e "PRs da branch". Com o gh (a linha de comando do GitHub) instalado e logado, a faixa mostra o PR aberto, rascunho ou incorporado e o resultado dos checks (ok, falhou, rodando), com o botão "Ver o PR". Só aparece com um origin e a tarefa numa branch diferente da base.
- **A conversa da abelha fixa lê como conversa:** as linhas do terminal viram balões, o que você mandou à direita e as frases do agente à esquerda. As chamadas de ferramenta (leu, procurou, editou, rodou) viram uma linha cinza entre os balões, que abre no clique com o começo de cada resultado. O que o agente pede vem no último balão, com a borda âmbar, o comando em mono e a explicação, e as opções seguem como botões. A janela cresce puxando pelo canto livre, a partir de 360 por 470, e lembra o tamanho por perfil. Segue o jeito do Claude Code de escrever; outros agentes aparecem como frases e notas.
- **Enquanto você esteve fora, pela abelha fixa:** sem mexer no computador por 15 minutos, ao voltar a abelha mostra um cartão com o que aconteceu nesse tempo e ainda vale: agentes que terminaram (com a frase de fim), tarefas concluídas, quem ficou esperando você e os erros. Os itens são os do resumo: a linha abre a conversa, a bolinha leva à Colmeia. O cartão fica até "Entendi" ou até a Colmeia voltar à vista. Só no X11, onde dá para saber há quanto tempo você não mexe no teclado nem no mouse.
- **Conversar com o agente pela abelha fixa:** clicar numa tarefa do resumo abre a conversa ali mesmo, com as últimas linhas do agente em texto, botões para as opções numeradas que ele oferece e uma caixa para responder ou pedir. "Abrir na Colmeia" continua para o terminal inteiro. Com o agente parado (depois de reiniciar o núcleo, por exemplo) a conversa abre do mesmo jeito, com "Retomar o agente", que liga ele de novo na sessão em que parou; uma tarefa sem agente ganha "Rodar com o Claude Code".
- **Tarefa nova pela abelha fixa:** "+ Tarefa" no resumo cria a tarefa no projeto escolhido, já com um Claude Code se quiser, e abre a conversa nela. O cartão tem o projeto numa linha inteira (a seta passa ao próximo), uma caixa de marcar para o Claude Code e o título embaixo.
- **Resumo da abelha fixa como a lateral:** cada item que pede você virou uma linha como as tarefas no cartão da lateral, o ponto de estado, o título da tarefa e, embaixo, o projeto com o que aconteceu. Os botões Abrir e Ver o erro saíram; no lugar, a bolinha do agente com a inicial da ferramenta e o anel na cor do que ele pede. A linha abre a conversa ali mesmo; a bolinha, com a marca de abrir fora no canto, leva à Colmeia já no agente. A tarefa sem agente diz "esperando você, sem agente" e fica com o anel vazio para alinhar. O cartão ficou mais baixo. Na conversa, as opções numeradas ficaram todas no mesmo botão (a primeira costuma aprovar um comando, e a cor da marca puxava o clique sem leitura) e, com outras tarefas esperando, "Próximo ›" no cabeçalho leva à seguinte.

- **Tirar uma tarefa da sprint:** o X da linha ou o botão direito tiram a tarefa só daquela sprint: ela sai da página, do texto copiado, da apresentação e do que a IA lê; a daily, o quadro e a linha do tempo continuam com ela. A lista "Tiradas desta sprint" traz de volta.
- **A fala de cada assunto:** com o mouse no cabeçalho do assunto, "+ Texto" abre a caixa para escrever a fala dele na reunião, e "Gerar texto" pede ao Claude Code um texto curto só com as tarefas do assunto que ficaram na sprint. A fala fica entre o cabeçalho e as tarefas, entra no texto copiado e dá para editar, gerar de novo ou apagar.

### Melhorado

- **O agente apaga da lousa:** ele só acrescentava, e ao pedir "limpa a lousa e monta de novo" ele respondia que os itens antigos você teria de apagar na tela. Agora tem a ferramenta `apagar_da_lousa`, pelo id ou tudo, com as ligações saindo junto; a tela mostra "Claude Code (dev) apagou 3 itens da lousa" e a linha do tempo conta. Mover e editar continuam fora, que é o que briga com a edição aberta.
- **A frase do agente na abelha com a estrutura que ele escreveu:** a conversa da abelha fixa juntava tudo num bloco só. Agora cada parágrafo fica separado, os itens de lista ganham a bolinha, uma linha curta sem pontuação vira título em seminegrito, `**negrito**` e `` `código` `` são desenhados em vez de aparecer com os marcadores, e o aviso "Jump to bottom" que o Claude Code sobrepõe ao texto rolado fica de fora.
- **A abelha com tema próprio e letra para os dois lados:** o "Aa" da conversa abre os ajustes da abelha: a letra vai do menor ao maior com − e + (Ctrl − e Ctrl + continuam), e o tema da abelha é escolhido ali, independente do da Colmeia, entre Escuro, Claro, Leitura e Fosco. A letra base da conversa ficou um pouco maior e o texto do agente mais claro no escuro. Os dois ficam guardados por perfil.
- **Tabelas na conversa da abelha:** uma tabela que o agente desenha no terminal (ou que você cola na mensagem) aparecia como uma fileira de quadradinhos, porque a letra da abelha não tem os traços de caixa. Agora a abelha reconhece a tabela e a desenha em grade, com o cabeçalho destacado, as colunas na largura do conteúdo e o texto quebrando dentro da célula quando não cabe; a linha que o terminal quebrou em duas volta a ser uma só. Os traços que sobram num texto comum saem na fonte de reserva, não mais em quadradinhos.
- **Esc fecha o cartão da abelha:** o resumo, o balão e o quadro da volta fecham com o Esc, não só pelo “x” ou por outro clique na abelha. Na conversa e na tarefa nova, o Esc faz o que o “x” faz (volta ao resumo); num menu aberto ou numa caixa de texto com foco, o primeiro Esc só fecha o menu ou sai da caixa, e o seguinte fecha o cartão. O clique no cartão traz o foco do teclado para a abelha, que é o que faz a tecla chegar.
- **Trocar de tarefa pelo chip da conversa:** o chip com o título, no topo da conversa da abelha, ganhou uma seta e abre a lista de todas as tarefas abertas (as que esperam você primeiro, depois as trabalhando, com o projeto e o estado de cada uma); clicar troca a conversa, sem precisar passar de uma em uma pelo "Próximo".
- **Concluído em azul:** a coluna Concluído, o selo da tarefa e o botão na caixa "Você parou o agente" tinham a mesma cor da Revisão. Agora o Concluído é azul, nos quatro temas, e na abelha também.
- **Letra maior na conversa da abelha fixa:** o botão "Aa" no rodapé da conversa passa pelas cinco escalas (100, 115, 130, 150 e 175%) e Ctrl + e Ctrl − (ou Ctrl com a rodinha) sobem e descem uma; Ctrl 0 volta à normal. A escolha fica guardada por perfil.
- **Botões de verdade na caixa "Você parou o agente":** as colunas viraram botões cheios na cor de cada uma, com o texto escuro, como os botões do cartão da abelha; "Deixar como está" ficou cinza e "Excluir tarefa" em vermelho, como nas outras caixas.

- **A sprint em faixas:** cada tarefa virou uma faixa translúcida com o título e o estado numa coluna à esquerda e a frase à direita, com a cor do estado na borda. O olho acha cada tarefa pelo título alinhado, em vez de um bloco de texto colado no próximo. Com o mouse em cima aparecem o lápis, "Apresentar a partir desta tarefa" e o X.
- **Destinos coloridos ao parar o agente:** no "Você parou o agente. O que fazer com a tarefa?", cada coluna aparece na cor dela no quadro.

- **A frase da reunião como você escreveu:** na daily e na sprint, uma frase de várias linhas aparece embaixo do título com os parágrafos, as seções em negrito e as listas, em vez de virar uma linha só.
- **Notas resumidas:** cada nota mostra o começo, com negrito e código desenhados em vez dos símbolos, e "Ver tudo" abre o resto. As sugestões dos agentes continuam inteiras, para você ler antes de aprovar.
- **Trocar de workspace leva a página junto:** clicar em outro quadrado do trilho mostra a mesma página dele (a lousa ou o conhecimento) ou o quadro dele, sem ficar no workspace anterior.
- **Projetos na barra lateral em linha ou cartão:** fechado, o projeto é uma linha com a inicial num quadradinho, o nome e um ponto à direita quando algo pede você (vermelho com erro, âmbar com espera) ou um verde com agente trabalhando. Aberto, vira um cartão com uma etiqueta de situação no cabeçalho ("1 com erro", "2 esperam você", "1 rodando"), as tarefas em andamento dentro e, no pé, o que ficou em revisão e concluído. Os agentes deixaram de ser um nível da árvore: viram bolinhas com a inicial da ferramenta e o anel na cor do estado na linha da tarefa, e a bolinha abre a tarefa já no agente. Saíram o número da tarefa, as contagens soltas e o "parado" das linhas. Dentro do cartão, a tarefa que pede você diz o que pede e desde quando ("pede aprovação · 14:32") antes das bolinhas, com o título em negrito e o ponto maior; a bolinha de quem pede você tem o anel mais grosso, quem pede vem por cima, e com mais de quatro agentes aparecem três e "+N". No cabeçalho, o segundo número em cinza antes da etiqueta ("3 esperam · 1 com erro") quando cabe, e "trabalhando" contando tarefas, como "esperam". Fechar um cartão à mão agora fica fechado até ele voltar a rodar (antes a escolha invertia quando os agentes terminavam). O cartão abre sozinho só com algo pedindo você ou trabalhando, a seta tem o lugar dela sempre e aparece com o mouse, e o caminho do projeto saiu da dica e foi para o menu do botão direito.
- **Árvore da barra lateral mais clara:** todo projeto tem o mesmo peso, aberto ou não, as tarefas entram mais e uma guia desce do projeto até o que é dele, com espaço entre um projeto e outro.
- **Cartão da abelha fixa mais caprichado:** a mesma fonte da Colmeia, botões com tamanho e cantos iguais, "Desafixar" mais discreto, a tarefa em destaque alinhada com o botão dela e um fio separando os botões do resto.
- **Resumo da abelha fixa com a tarefa na frente:** cada item mostrava "Claude Code (dev) espera sua resposta" em destaque e cortava o título da tarefa e o projeto. Agora a tarefa vem em cima, o projeto e o que aconteceu embaixo, em três linhas, o cartão ficou mais largo e o item inteiro abre a tarefa.
- **Imagem do workspace sem sobra:** a margem transparente e a sombra em volta de um ícone saem ao gravar, e a imagem ocupa o quadrado do trilho inteiro. As imagens já escolhidas também saem aparadas.
- **Dois cliques abrem o ramo:** no projeto ou na tarefa da árvore, além da seta.
- **A logo leva para todos os projetos.**

### Novo

- **Renomear tarefa:** no botão direito do cartão (ou no "⋯") e, na daily, editando a frase, o título também muda. O título novo vale no quadro, na daily, na sprint e no histórico; a branch e a pasta não mudam.
- **Concluir direto do cartão:** o menu do cartão tem "Concluir" em qualquer coluna, sem passar pela Revisão.
- **Base de conhecimento por workspace e por projeto:** em "Conhecimento", na barra lateral, você escreve uma vez o que explicaria para todo agente (onde ficam os logs, como um nome é gerado, que projeto olhar junto), e todo agente que começa ali já recebe, sem você repetir. As notas do workspace valem para todos os projetos dele. Os agentes também sugerem notas, que só valem quando você aprova, edita ou recusa, com um selo mostrando quantas esperam. Sem notas, nada muda e nenhum token é gasto.
- **Editar o workspace:** o lápis no topo do painel (ou o botão direito no quadrado) troca o nome e põe qualquer imagem como ícone. O meio da imagem vira o quadrado do trilho.
- **Arquivos nas notas de conhecimento:** uma nota leva até 10 arquivos de até 25 MB, como o PDF do passo a passo ou um modelo. Arraste para a página ou use "Anexar arquivo" no editor. O agente recebe só o caminho e lê quando precisar, então o arquivo não gasta tokens à toa. Soltar arquivos na página sem nota aberta começa uma nota com eles.
- **Concluir sem revisor:** a tarefa em Revisão mostra "Concluir" no cartão e no topo dela, e a que tem agente trabalhando ou esperando você mostra "Enviar para revisão". Quando você pede ao Claude Code para concluir ou mandar para revisão, ele mesmo move a tarefa, e o histórico diz que ela foi movida pelo agente a pedido. Ele nunca faz isso sozinho.
- **Whisper small:** em Ajustes, "Modelo de transcrição" troca o Whisper base pelo small, com 488 MB, mais preciso e mais lento, baixado uma vez e conferido antes de usar. Com só um dos dois no computador, vale ele.

- **Pergunta livre com IA:** uma opção à parte em Ajustes, desligada porque cada pergunta gasta tokens. Ligada, o que você escreve na caixa nova do resumo da abelha, na janela e na abelha fixa, vira uma pergunta sobre o andamento, como "o que eu fiz hoje?" ou "o que falta na sprint?". O Claude Code responde em poucas frases, no balão da abelha, olhando só as tarefas, os agentes, as frases da reunião e o que aconteceu hoje. O terminal, as senhas e os bancos não vão, e nada fica guardado. Sem o Claude Code instalado, a abelha avisa.
- **Ditado por voz:** em Ajustes, "Ditado por voz" vem desligado. Ao ligar, a Colmeia mostra o tamanho do modelo (148 MB) e baixa uma vez o Whisper base, conferido antes de usar, com barra de progresso e Cancelar. Ligado, um microfone aparece ao lado do Enviar na caixa de mensagem da tarefa. Clique para falar e de novo para parar, ou segure Ctrl+Shift+M. O botão fica vermelho enquanto ouve. O texto entra na caixa para você revisar e nunca é enviado sozinho. Tudo roda no computador, em português, sem internet e sem tokens, e o áudio não é gravado. O ditado é um programa à parte que abre só quando você usa e sai sozinho depois de 2 minutos parado.
- **Abelha fixa na área de trabalho:** em Ajustes, "Fixar a abelha na área de trabalho" abre a abelha num programa à parte, pequeno e sem moldura, por cima das outras janelas e em todas as áreas de trabalho. Ela continua lá com a janela da Colmeia fechada. Parada, mostra um ponto da cor do que mais pede você, e um contador vermelho quando há erro. O clique abre o resumo, que fica até você fechar, com quantos esperam você, trabalhando e com erro, a tarefa que pede você e os botões Abrir a Colmeia e Desafixar. Quando algo acontece, um balão sai dela e some em 8 s, parado enquanto o mouse está em cima. Abrir e Ver o erro trazem a Colmeia já na tarefa, abrindo a janela se ela estiver fechada. Dá para arrastar e ela lembra onde ficou. No Wayland ela é uma janela comum.
- **A abelha do seu jeito:** cada perfil escolhe o acabamento da abelha entre cinco (Colmeia, Clássica, Redonda, Favo e Zangão, este mais robusto, com corpo oval âmbar queimado, olhos grandes, asas maiores e um tufo no lugar das antenas), todos com os mesmos estados, e dá um nome a ela. O nome, Colmeia por padrão, aparece no pé da barra lateral, no resumo, nos balões e na abelha fixa. Fica em Ajustes, "Abelha", com a prévia de cada uma.
- **Bancos no visual novo, com o pedido do agente por inteiro:** na tela de bancos, o painel da barra lateral mostra no topo os pedidos dos agentes esperando você e, embaixo, as conexões por pasta com o filtro. O console ganhou o nome da conexão em destaque, a barra com banco, limite, tempo e o modo, e o resultado com o tipo embaixo do nome de cada coluna, listras, a contagem, o tempo e a hora, e Copiar tudo. O pedido abre numa gaveta com quem pede, a tarefa, a conexão, o prazo e a instrução travada. "Rodar para mim antes" roda a instrução no seu console, só leitura, e mostra o que o agente vai receber, e embaixo aparece o que esse agente já pediu ali. Recusar leva o motivo, se você escrever um.
- **Autocompletar no console de bancos:** sugere as palavras do SQL do banco e os nomes de esquemas, tabelas e colunas que a árvore já abriu, num menu leve embaixo do cursor. Depois de "p." ele mostra as colunas da tabela de apelido p, buscadas uma vez só. Tab ou Enter aceita, Esc fecha e Ctrl+Espaço chama. Tudo local, sem IA, e sempre ligado: como não gasta nada, não é uma opção.
- **Daily e sprint para ler em voz alta, e Gerar com IA:** a daily virou uma coluna só, com Ontem, Hoje e Esperando minha resposta, e cada tarefa é uma linha com o estado, o título e a frase que o agente deixou. O lápis aparece com o mouse em cima e edita na própria linha, e "Escrevendo a frase…" mostra quando o agente ainda está escrevendo. Na sprint, cada assunto é uma seção que recolhe, com "Sem assunto" no fim, e a tarefa muda de assunto arrastada até a seção, mesmo recolhida. Saíram a fileira de números, os cartões, o Resumo e Detalhes e o "Pedir ao agente" de cada item; abrir a tarefa e ver na apresentação ficaram no botão direito. Gerar com IA pede ao Claude Code, sem terminal, títulos, descrições e assuntos do jeito que você disser. A proposta aparece ao lado de como está e nada muda até você aceitar. Usa tokens da sua conta e manda só o que está na sprint.
- **Tarefa em abas:** a tarefa abre no Terminal, com os agentes em fichas em cima dele. Cada ficha mostra o estado, a última linha e, no Claude Code, o contexto usado num anel. A ficha escolhida se junta ao terminal, a caixa de mensagem fica no pé dele com Um agente ou Todos, e o mosaico continua a um clique. As outras abas são Arquivos, Lousa, Navegador e Play, Descrição e Linha do tempo, com Ctrl+1 a Ctrl+6. Em Arquivos dá para buscar pelo nome e pelo texto do código, ver o arquivo com o número da linha e o que o agente mudou contra a branch base, sem editar. O botão abre no IntelliJ, na linha. A Descrição mostra o que o agente escreveu ao terminar, com quem e quando, e a Linha do tempo só o que aconteceu nessa tarefa. Trocar de aba não muda o tamanho do terminal.
- **Quadro com abas e Histórico:** o topo virou abas, Quadro, Histórico, Daily e Sprint. Ao lado do nome do projeto, o resumo diz quantas tarefas esperam você, quantas estão trabalhando e quantas deram erro, e o clique destaca só essas no quadro. As colunas ganharam ícone e um + para criar a tarefa já nelas, e "Aguardando você" fica em âmbar quando tem cartão. A etiqueta do cartão sai do começo da branch (funcionalidade, correção, docs, main) e a marca "sozinha" mostra o caminho que a tarefa faz sozinha. O Concluído mostra só o que terminou hoje, com um link para o resto. O Histórico substitui a linha do tempo: as concluídas por dia, com busca, e ao lado o que aconteceu em cada uma, com Reabrir e Abrir tarefa. A tarefa que vai sozinha para Revisão e Concluído agora aparece no histórico, na daily e na sprint.
- **O agente escreve a frase da reunião ao terminar:** as instruções que a Colmeia dá ao Claude Code agora pedem que, antes de encerrar, ele grave na daily uma ou duas frases sem termos técnicos, dizendo o que ficou pronto e para quem. Se ele gravou, a Colmeia não pede de novo e só leva a frase para a sprint. A frase montada fica como reserva.
- **A frase da reunião se escreve sozinha:** quando a tarefa vai para Revisão ou Concluído, a Colmeia pede ao Claude Code da tarefa uma frase curta, sem termos técnicos, e guarda na nota da daily e da sprint. O cartão mostra "Escrevendo a frase…" enquanto isso. Se nenhum agente estiver rodando ou ele não responder em uns minutos, entra uma frase com o título e o seu último pedido. O que você já escreveu na nota nunca é trocado.
- **A tarefa anda sozinha até o fim:** quando todos os agentes da tarefa terminam bem, ela vai para Revisão. Quando um agente com o papel revisor termina bem, vai para Concluído. Sem revisor, fica em Revisão. Mover o cartão à mão continua mandando. Se você para um agente pela tela, a Colmeia pergunta o que fazer com a tarefa: mover para uma coluna, excluir ou deixar como está.
- **Uso de verdade no pé da barra lateral:** os agentes do Claude Code abertos pela Colmeia mandam o próprio uso. A barra mostra o contexto do agente em foco e quanto da janela de 5 horas e da semana da conta já foi, com a hora em que volta na dica. A Colmeia põe isso só nas sessões dela, sem mexer na sua configuração do Claude Code. As outras ferramentas continuam sem dados.
- **Sprint com datas fixas e Resumo para a reunião:** a sprint deixou de ser "os últimos 14 dias", que mudava todo dia e levava junto o que você tinha escrito. Agora ela tem começo e fim: a primeira vai de sexta a quinta, o dia da reunião, e as seguintes nascem sozinhas no mesmo ritmo. As datas são editáveis e as setas voltam às sprints anteriores. A página abre em **Resumo**, uma lista com o título e a descrição de cada tarefa, editáveis pelo lápis só nesta sprint ("Comparar bancos da loja 5" pode virar "Entrada da loja Alphaville 5"); a tarefa, o quadro e a daily continuam como estão, e a apresentação usa o título da sprint. "Ver detalhes" abre o cartão completo embaixo do item, e **Detalhes** mostra os cartões de sempre.
- **Assuntos na sprint:** juntam tarefas de projetos diferentes para a reunião sem mudar o projeto delas. No Resumo, arraste uma tarefa ou um projeto inteiro pela alça até um assunto; soltar em "+ Novo assunto" cria um com aquele nome, e o botão direito renomeia ou apaga. "Repetir os da sprint anterior" traz os assuntos e as tarefas da semana passada. A lista, a apresentação (com o divisor de cada assunto) e o "Copiar texto" seguem os assuntos, e o texto copiado passa a usar o título da sprint. O "Pedir ao agente" de cada item sugere pedir uma descrição para a reunião, sem termos técnicos.
- **Rodar o projeto (Play), como no IntelliJ:** configurações de execução por projeto (comando do shell, pasta e variáveis de ambiente), trazidas do IntelliJ (Go, Go test, Flutter, npm e Shell) ou sugeridas pelo `package.json`, `Makefile`, Cargo e Go. O botão Rodar fica no quadro do projeto e no topo da tarefa (roda na pasta dela); a saída aparece num painel embaixo, com Parar e Rodar de novo. Quem roda é o núcleo, então fechar a janela não para o programa. Uma execução que sai com erro avisa. Shift+F10 roda e Ctrl+F2 para.
- **Copiar do terminal:** arrastar com o mouse seleciona, dois cliques pegam a palavra (um link ou caminho vem inteiro) e três a linha. Ctrl+C copia o que está selecionado (sem seleção, continua interrompendo o programa), assim como Ctrl+Shift+C e o "Copiar" do botão direito. Vale para os terminais dos agentes e para a saída do Play.
- **Tema fosco:** um quarto tema, cinza-carvão sem brilho, com contraste mais baixo que o escuro e cores de estado apagadas. Fica no menu de tema da barra lateral, na criação do perfil e em `COLMEIA_TEMA=fosco`. Um banco antigo é ajustado sozinho na primeira abertura.
- **Tirar uma tarefa da daily de hoje:** pelo menu do cartão ("Tirar desta daily") ou pelo H no modo apresentação da daily. Serve para o que já foi apresentado na reunião anterior. A tarefa sai do deck e do texto da daily daquele dia, fica listada no fim da página com "Trazer de volta", e continua na sprint e na linha do tempo. A daily de outro dia não muda.

### Mudou

- **Ouvir o nome mais leve e mais fácil de acertar:** gasta menos processador em silêncio e falando, a escala da sensibilidade ficou mais larga e o modelo de transcrição sai da memória quando fica parado, mesmo com a abelha ouvindo.
- **Abelha fixa mais limpa:** um indicador só no canto, com "!" para um aviso e o número para mais. O balão mostra o título inteiro em até duas linhas, o detalhe legível e botões maiores. Com o microfone aberto a abelha põe um fone com o microfone aceso, nos cinco modelos. Ela se esconde enquanto a janela da Colmeia está à vista e volta no mesmo lugar, não some no "mostrar a área de trabalho", e dois cliques seguidos não a movem mais.
- **Barra lateral sem vão:** num perfil sem projeto o painel fica recolhido e sobra só o trilho. O contador dos workspaces e dos bancos cabe inteiro no trilho.
- **Identidade nova:** a logo agora são três células do favo que formam uma abelha vista de cima, também no ícone do atalho e da janela. Os quatro temas ganharam cores revisadas, com todo texto legível em qualquer fundo, e o destaque do Leitura passou de ferrugem para ameixa. O conteúdo fica num cartão arredondado sobre um gradiente leve, botões e itens mudam de cor suavemente ao passar o mouse, e a abelha ganhou o acabamento da logo (corpo em célula, asas de favo e tela com moldura).
- **Barra lateral nova:** à esquerda fica um trilho fino com a logo e cada workspace num quadrado com a inicial, na cor que sai do nome, e um contador âmbar de quantos esperam você. Ao lado abre o painel do workspace, com os projetos, as tarefas em andamento e os agentes em árvore: o projeto abre o quadro, o agente abre a tarefa já no terminal dele. A lousa do workspace continua no painel e os bancos de dados, os atalhos, os ajustes, o perfil e o tema ficam no trilho. O painel recolhe e abre com uma animação curta. No pé dele, a abelha com o estado e as três barras do uso (contexto, 5 h e semana). Os workspaces não recolhem mais na lista: o trilho mostra todos.
- **Ctrl+Enter quebra a linha** na caixa de mensagem, no "Pedir ao agente" e no terminal do agente, como o Shift+Enter. Só o Enter puro envia, como no terminal do Claude Code.
- **Daily e sprint carregam juntas:** quem atualiza uma encontra a outra já pronta, em vez da página vazia esperando a resposta.

### Corrigido

- **A abelha fixa volta com a janela minimizada:** antes ela só reaparecia ao fechar a janela. Agora volta também quando a janela da Colmeia é minimizada ou fica em outra área de trabalho, e some de novo quando a janela volta.
- **O balão da abelha fixa ficava na tela** vazio depois do tempo dele.
- **A nota do slide aparece inteira:** passava de 14 linhas e era cortada (com "…" em cima quando o agente tinha respondido um pedido). Agora usa o espaço livre e, se ainda não couber, rola dentro dela; respondido um pedido, começa no fim.
- **A daily e a sprint fechavam a Colmeia** quando o nome do projeto (ou do recorte) era curto, como "loja".
- **Terminal embaralhado ao reabrir a Colmeia:** os terminais conectavam antes de a tela saber o tamanho deles, e o histórico chegava desenhado em 80 colunas. Agora, se o primeiro tamanho em foco for outro, o terminal reconecta já no tamanho certo e pede ao programa que redesenhe.
- **Recolher um workspace na barra lateral** só escondia os projetos quando o mouse mexia; agora esconde na hora.

## [0.4.0] · 06/10/2026

### Entrega I · Windows

#### Novo

- **A Colmeia roda no Windows 10 (1803 ou mais novo) e 11.** Núcleo, terminais dos agentes, navegador da tarefa, bancos de dados (senhas no Gerenciador de Credenciais) e a tela, com instalador `instalar.ps1` e o pacote `colmeia-windows-x86_64.zip` na release ([decisão 0011](docs/decisoes/0011-windows-com-socket-unix.md)).
- **Canal:** o mesmo socket Unix do Linux (AF_UNIX), em `%LOCALAPPDATA%\Colmeia\canal`, com lista de acesso protegida só do usuário e do sistema; os dados ficam em `%LOCALAPPDATA%\Colmeia\dados`, com a mesma proteção.
- **Terminais pelo ConPTY**, com o programa num job object: fechar o agente avisa o programa e, depois de 3 s, encerra a árvore inteira. O `claude` instalado pelo npm (`claude.cmd`) roda pelo `cmd.exe`; o agente "shell" abre o PowerShell.
- **Navegador:** o Chrome (ou o Edge, que vem com o Windows) com o mesmo pipe de controle, sem porta de rede.
- Testes do Windows no CI (canal e lista de acesso, ConPTY, caminhos e o bloco de descritores do navegador) e espera da thread do terminal pelo WSAPoll.

#### Mudou

- A conferência de "dentro da pasta" (arquivos da tarefa e `file://` do navegador) usa o separador do sistema e recusa caminhos que começam na raiz da unidade ou em outra unidade.

### Corrigido

- **Ligar notas pela bolinha na lousa:** as bolinhas de ligação ficavam a 20 px da nota e sumiam no caminho até elas, porque o mouse saía da nota. Agora continuam à vista enquanto o mouse está perto da nota.

## [0.3.0] · 05/10/2026

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
- **Barra lateral com o workspace em destaque:** o nome do workspace em seminegrito e na cor do texto, a seta na cor de destaque, os projetos recuados embaixo dele e uma linha fina entre um workspace e outro.
- **Caminho do topo clicável:** o perfil abre todos os projetos, o workspace abre a visão dele e, dentro de uma tarefa, o nome do projeto volta ao quadro. Da lousa e dos bancos, o clique volta ao quadro.
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
