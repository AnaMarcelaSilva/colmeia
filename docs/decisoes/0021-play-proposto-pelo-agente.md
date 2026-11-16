# 0021 · O Play proposto pelo agente e visto por workspace

**Decidido em 27/10/2026.**

## Contexto

A usuária pediu a um agente "cria um play padrão aqui na Colmeia desses dois, tanto da api quanto do app". O agente não tinha como: o Play (as configurações de execução, decisão de 20/10) era só da tela, por escolha, porque é um comando de shell que roda na máquina dela pelo botão Rodar. Ele fez o que dava, uma nota na base de conhecimento, e ela perguntou por que o Play não foi criado. Ela quer que o agente consiga criar, e que o Play apareça por workspace, não só por projeto: a API e o app são dois projetos do mesmo workspace e sobem juntos.

## Decisão

- **O agente propõe; ela aceita.** A ferramenta `propor_play` (e `listar_plays`, para não repetir) grava a configuração com origem `agente` e `proposta` ligada (versão 14 do banco: a origem entra no CHECK e a tabela `comandos` ganha `proposta`, `proposta_por` e `proposta_tarefa`). Uma proposta aparece em "Propostas do agente" nas Configurações de execução, com nome, comando, pasta e os nomes das variáveis à vista, e os botões Aceitar, Editar e Descartar. Rodar uma proposta é recusado pelo núcleo (também pelo comando falado) até `POST /v1/comandos/{id}/aceitar`. A tela não cria com origem `agente`.
- **Sem segredos na proposta.** Comando e valores de variável passam pelo mesmo filtro das notas; o que parecer segredo é recusado com a instrução de deixar o valor vazio para a usuária preencher. Na lista que o agente lê, as variáveis vêm só pelo nome: o valor pode ser um segredo dela.
- **O projeto é o da tarefa ou outro do mesmo workspace.** O agente passa `projeto` pelo nome; fora do workspace é recusado, com a lista dos nomes válidos. O Play continua pertencendo a um projeto (ele roda numa pasta), e por isso não existe "Play do workspace" como entidade: o que existe é a lista por workspace, `GET /v1/workspaces/{id}/comandos`, com o nome do projeto em cada linha.
- **O menu do Rodar mostra o workspace inteiro.** Embaixo das configurações do projeto em foco, o menu traz as dos outros projetos do workspace, agrupadas pelo nome; cada uma roda na pasta do próprio projeto (não na da tarefa), e a saída vai para o mesmo painel. Na aba da tarefa o painel mostra a última execução, seja de que projeto for.
- **Na linha do tempo e na tela.** A proposta (`comando.proposto`, com o agente e a tarefa) e o aceite (`comando.aceito`) são eventos da corrente e aparecem na linha do tempo ("Claude Code (dev) propôs o Play “api” em loja-api"). A tela recebe os dois como avisos: a proposta vira um balão dizendo onde aceitar; os dois fazem a lista do projeto ser buscada de novo.

## Alternativas descartadas

- **Criar direto, sem aceitar.** Seria a entrega mais simples, mas um comando com nome inocente e conteúdo ruim rodaria ao primeiro clique no Rodar. Com o aceite, o comando é lido antes de existir como botão, e a origem fica guardada.
- **Um Play do workspace que roda vários projetos.** Daria para "subir tudo" num clique, mas cada processo tem a sua pasta, a sua saída e o seu Parar; juntar num só esconde qual quebrou. A lista por workspace resolve o pedido (subir os dois de um lugar) sem criar outro tipo de coisa.

## Consequências

- O agente responde "proposto; aceite nas Configurações de execução" em vez de "não tenho ferramenta". O SECURITY.md registra que o agente só propõe.
- Bancos antigos passam pela migração 14 ao abrir; o que já estava gravado continua, com proposta desligada.
