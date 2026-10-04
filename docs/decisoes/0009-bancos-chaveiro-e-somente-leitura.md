# 0009 · Bancos de dados: senha no chaveiro, só leitura por padrão e o agente só com aprovação

**Decidido em 03/10/2026.**

## Contexto

A usuária quer, por perfil, conexões de banco como as de uma IDE: criar, testar, navegar até as colunas, rodar SQL num console e ver o resultado numa grade. Quer também que o agente da tarefa possa olhar dados de um banco quando precisar.

Uma conexão de banco tem senha, os dados dela podem ser de produção e o agente manda o que lê para o provedor de IA. A Colmeia já guarda tudo num SQLite local, com um histórico imutável encadeado por hash, e o agente fala com o núcleo por ferramentas MCP restritas à tarefa.

## Decisão

- **A senha nunca entra no banco da Colmeia.** Ela vai para o chaveiro do sistema (Secret Service pelo D-Bus), com uma conta aleatória por conexão (`conexao:<uuid>`), e a tabela guarda só onde ela mora (`chaveiro`, `memoria` ou `nenhuma`). Sem chaveiro, fica só na memória do núcleo até ele encerrar, e a tela pergunta ao conectar. A falta do chaveiro é estado da sessão, não da conexão: uma conexão do chaveiro continua marcada `chaveiro` mesmo quando ele está bloqueado ou fora do ar, e no próximo início a senha volta a ser lida dele. Só vira `memoria` quando você desmarca "Guardar no chaveiro" com ele disponível. Ela nunca volta numa resposta, num evento, num aviso ou no log; os erros dos drivers passam por um redator.
- **A configuração da conexão é toda explícita.** O ambiente do usuário (`PGHOST`, `PGPASSWORD`, `~/.pgpass`, `PGOPTIONS`, certificados de cliente) não muda a conexão que a Colmeia abre, e o `LOAD DATA LOCAL INFILE` do MySQL fica desligado: um servidor malicioso não pede arquivo local.
- **Só leitura por padrão, em três camadas.** O classificador léxico decide se a instrução lê ou altera (na dúvida, altera). A leitura roda numa transação só de leitura sempre desfeita (ou com o SQLite aberto só para leitura). Escrever exige ligar "Permitir alterações" na conexão e confirmar cada instrução, com um nonce de uso único preso ao SQL exato, que vale por 2 minutos.
- **Uma instrução por execução.** Várias instruções num envio são recusadas; o console executa a seleção ou a instrução sob o cursor.
- **O agente só consulta, e só com a sua aprovação.** A conexão precisa de "Agentes podem pedir consultas" (desligado por padrão). Cada pedido mostra a instrução no painel da tarefa e segura o agente em "aguardando · aprovar consulta" até você aprovar ou recusar; sem resposta em 5 minutos, expira. Mesmo numa conexão com escrita, o pedido do agente roda em leitura, sem as funções que leem arquivos do servidor ou derrubam sessões, com até 200 linhas e 64 KB de resposta. Não há aprovação automática.
- **A corrente registra que houve, nunca o quê.** `banco.consulta` e `banco.alteracao` têm a conexão, o verbo, as linhas e o tempo, sem SQL nem resultado. O SQL fica no histórico de consultas da conexão, fora da corrente e apagável, como as mensagens (decisão 0006); o que parece levar uma senha não é guardado. O pedido do agente, que leva o SQL, só vai para a tela.

## Alternativas

- **Senha cifrada no SQLite** com uma chave derivada de algo da máquina: a chave ficaria ao lado dos dados, e a cópia do banco levaria as senhas. O chaveiro do sistema já resolve, com o desbloqueio do usuário.
- **Confiar só na transação de leitura:** o DDL do MySQL faz commit sozinho e escapa da transação, e o SQL Server não tem transação só de leitura. Por isso o classificador vem antes, e a transação e o `ROLLBACK` vêm depois.
- **Confiar só no classificador:** um analisador léxico erra; a transação de leitura do banco é a segunda barreira. A terceira, recomendada na documentação, é um usuário de banco só de leitura.
- **Deixar o agente escrever com confirmação:** a usuária confirmaria instruções que não escreveu, dentro de um fluxo que o agente controla. Fica para depois, se fizer falta.
- **Aprovar "sempre para esta conexão":** tiraria a usuária do caminho justamente quando o agente está em loop. Recusado nesta entrega.
- **Guardar o SQL das consultas na corrente:** o texto ficaria imutável para sempre, inclusive com dados colados por engano.

## Consequências

- A primeira conexão pode abrir a janela de desbloqueio do chaveiro; a chamada tem prazo de 30 s e, se ele não responder, a senha fica só na memória naquela sessão.
- "Só leitura" não é uma fronteira completa: no Postgres, funções com efeito colateral rodam numa transação só de leitura (`pg_terminate_backend`, `set_config`). A documentação recomenda um usuário de banco só de leitura.
- O agente espera a resposta por minutos: o cliente MCP tem prazo por chamada (6 min na consulta) e a configuração MCP pede ao Claude Code um `timeout` de 7 min.
- O SQL Server fica "experimental": o classificador e a montagem da configuração têm testes de unidade, mas a integração com um servidor de verdade não roda no CI.
