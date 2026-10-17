# 0016 · Pergunta livre com IA, num pedido só e sem guardar nada

**Decidido em 16/10/2026.**

## Contexto

Os comandos falados ([decisão 0015](0015-comandos-falados-e-voz-da-abelha.md)) entendem seis pedidos fixos, por regras e sem tokens. Perguntas abertas sobre o andamento ("o que eu fiz hoje?", "o que falta na sprint?") não cabem em regras. A sprint gerada com IA ([decisão 0012](0012-sprint-gerada-pelo-claude-code.md)) já faz um pedido só ao Claude Code, sem terminal e sem ferramentas; a pergunta livre pode usar o mesmo caminho.

## Decisão

- **Opção à parte, desligada por padrão.** "Pergunta livre com IA" é um item próprio no menu de Ajustes (`voz_ia` no perfil), que avisa que cada pergunta gasta tokens da conta do Claude Code do perfil. Ligar é recusado sem o Claude Code no PATH.
- **Só o que não é comando.** A frase vai pela mesma rota dos comandos (`POST /v1/perfis/{id}/voz/comando`). O interpretador tenta primeiro; só o que ele não reconhece (intenção desconhecida) vira pergunta livre, e só com a opção ligada. Com a opção desligada, nada é chamado e a abelha diz o que dá para pedir.
- **Falada ou escrita.** A pergunta chega transcrita (com os comandos falados) ou escrita numa caixa no resumo da abelha, na janela e na abelha fixa. A caixa aparece com os comandos ou com a pergunta livre ligados; com só a pergunta livre, o clique na abelha continua abrindo o resumo e o microfone não abre.
- **O mesmo pedido da 0012.** O código comum saiu do pacote `proposta` para o `claudecode`: `claude -p --output-format json`, sem ferramentas, sem MCP, sem configurações de projeto ou de usuário, sem guardar a conversa, numa pasta vazia apagada depois, com saída de até 1 MB e o envelope conferido. Prazo de 1 minuto (a tela espera até 75 s, fora da thread da tela). Uma pergunta por perfil de cada vez.
- **Um retrato mínimo e só de leitura.** O núcleo monta: as tarefas do perfil (título, projeto, estado; as abertas primeiro, até 60), os agentes de cada uma (nome da ferramenta e estado, com o motivo de uma lista fixa), a última frase da daily e da sprint de cada tarefa e as linhas de hoje da linha do tempo, só dos tipos de andamento (criar, mover, concluir, sessões, erros, notas), até 60. Ficam de fora o conteúdo dos terminais, as mensagens e pedidos ao agente, os caminhos, o navegador, a lousa, as capturas e tudo dos bancos. Cada texto vai numa linha, cortado em 300 letras, e o que tem cara de senha, token ou chave não entra. A pergunta com cara de senha é recusada antes de sair.
- **Resposta curta e conferida.** O pedido diz que é para ser falada (uma a três frases, até 500 letras, sem markdown) e para usar só o retrato. O núcleo recusa a vazia, a com caractere de controle, a com cara de senha e a maior que 2.000 letras; tira os enfeites de markdown, junta as linhas e corta no fim da última frase que cabe em 500 letras.
- **Nada em disco.** A pergunta, o retrato e a resposta só passam pela memória do núcleo e da tela: nenhum evento, nota, log ou arquivo (a pasta do pedido é vazia e apagada). A resposta aparece no balão da abelha e, com a voz da abelha ligada, é falada como as outras. Sem o Claude Code, a abelha explica no balão; o que dá errado também vira frase do balão.

## Alternativas

- **API da Anthropic direto:** pediria uma chave à parte; o Claude Code já tem a conta da pessoa (a mesma razão da 0012).
- **Mandar a pergunta ao agente de uma tarefa:** gastaria o contexto dele e só veria aquela tarefa; e o terminal dele tem o conteúdo que o retrato deixa de fora.
- **Um retrato maior (as mensagens, o terminal, a semana inteira):** responderia mais, mas levaria ao provedor o que a Colmeia promete não levar, e cada pergunta ficaria mais cara.
- **Uma rota só para a pergunta:** a mesma rota dos comandos deixa o interpretador decidir antes, sem a tela saber o que é comando.

## Consequências

- Cada pergunta livre gasta tokens e leva alguns segundos; a tela mostra "Um instante…" e a resposta longa fica mais tempo no balão (o tempo de ler).
- A resposta depende do que o retrato tem: o que aconteceu antes de hoje só aparece pelas frases da reunião e pelo estado das tarefas.
- O retrato leva ao provedor de IA os títulos das tarefas, os nomes dos projetos e as frases da reunião.
