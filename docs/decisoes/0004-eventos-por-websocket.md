# 0004 · Eventos em tempo real por um WebSocket por perfil

**Decidido em 02/10/2026.**

## Contexto

Até a entrega B a tela relia projetos, tarefas e agentes depois de cada ação (1 + 2·N pedidos que bloqueavam a tela) e só sabia que um agente parou quando o terminal dele avisava. Nada chegava de fora: um agente que começa a esperar você, um erro, uma tarefa concluída por outro cliente. A regra de desempenho do projeto proíbe consulta periódica.

## Decisão

- Um WebSocket por perfil aberto, `GET /v1/perfis/{id}/eventos`, no mesmo canal local e com o mesmo token. A tela não manda nada por ele além do fechamento; o que chegar é descartado.
- Cada mensagem leva o objeto inteiro (a tarefa, o agente) e um número `seq`. Aplicar a mesma mensagem duas vezes não muda nada.
- Para não perder nada entre o retrato e as mensagens: a tela abre o WebSocket, pede `GET /v1/perfis/{id}/quadro` (que traz o `seq` já incluído) e aplica só o que vier com `seq` maior.
- As mensagens nascem no banco: os eventos gravados numa transação são publicados só depois do commit. Uma mudança desfeita não avisa ninguém. Só o estado do agente (trabalhando, aguardando, ocioso) vai direto ao barramento, sem passar pelo banco.
- Quem publica nunca espera. Cada tela tem uma fila de 256 mensagens; se ela ficar para trás, recebe um único `{"tipo":"recarregar"}` e pede o retrato de novo.
- No máximo 16 telas ao mesmo tempo.

## Alternativas

- **Consulta periódica:** simples, mas fere a regra de desempenho e atrasa o aviso.
- **SSE (`text/event-stream`):** serviria, mas o canal já fala WebSocket nos terminais, e o cliente da tela já tem um; dois mecanismos para o mesmo papel não compensam.
- **Um WebSocket por projeto:** a abelha e a linha do tempo olham o perfil inteiro; por projeto seriam várias conexões para a mesma tela.

## Consequências

- A tela parada não faz nenhum pedido: a thread de eventos dorme numa leitura sem tempo limite.
- Um cliente futuro (o celular) usa o mesmo protocolo, sem regra de negócio do lado dele.
- O protocolo continua na versão 1: tudo foi acréscimo. Uma tela nova com um núcleo antigo recebe 404 na rota e mostra como atualizar.
