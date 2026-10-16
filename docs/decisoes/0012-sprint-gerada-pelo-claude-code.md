# 0012 · Sprint gerada pelo Claude Code, sem terminal e só com aceite

**Decidido em 16/10/2026.**

## Contexto

A sprint precisa de títulos, descrições e assuntos que façam sentido para quem ouve a reunião ("agrupe por cliente e escreva para o gerente"). Reescrever tudo à mão é o que a pessoa quer evitar, e o pedido ao agente de cada tarefa (decisão 0007) cuida de uma tarefa por vez, dentro do terminal dela.

## Decisão

- **Um pedido só, sem terminal.** O núcleo roda o `claude` com `-p --output-format json`, sem nenhuma ferramenta (`--tools ""`), sem servidores MCP, sem configurações de projeto ou de usuário (sem ganchos) e sem guardar a conversa, numa pasta temporária vazia, apagada depois. O executável é o do PATH, chamado direto (nada por shell), com a conta que o perfil usa para o Claude Code.
- **Só os dados da sprint.** O pedido leva o texto da pessoa e um JSON com as tarefas do deck: título, projeto, estado, descrição, assunto e até 8 linhas do que aconteceu. Nada de código, caminhos ou terminal. O pedido com cara de senha é recusado.
- **Resposta estrita.** O envelope do `--output-format json` tem de ser um `result` de sucesso, e o resultado, o JSON pedido: campo desconhecido, texto depois, tarefa que não é da sprint, repetida, título ou assunto com quebra de linha, tamanho fora do limite, mais de 12 assuntos ou algo com cara de senha recusam a proposta inteira. Só um bloco de código em volta é tolerado. Prazo de 3 minutos e saída de até 1 MB.
- **Nada gravado sem aceitar.** A proposta fica na memória do núcleo (as 8 mais novas, por até 1 hora) e volta para a tela ao lado de como está. Aceitar manda só o número: o núcleo grava exatamente o que conferiu (o título só da sprint, a descrição na nota da sprint e o assunto, criando os que faltam pelo nome). Repetir o aceite é seguro. Uma geração por vez.

## Alternativas

- **Pedir ao agente de cada tarefa:** uma conversa por tarefa, mais tokens, e o agrupamento por assunto precisa ver a sprint inteira.
- **API da Anthropic direto:** exigiria uma chave à parte; o Claude Code já tem a conta da pessoa.
- **Gravar e deixar desfazer:** mais simples na tela, mas trocaria o que ela escreveu antes de ela ler.

## Consequências

- Usa tokens da conta do Claude Code do perfil; a tela avisa antes. Sem o Claude Code instalado, o botão fica inativo e explica.
- A geração prende um pedido HTTP pelo tempo do agente (a tela espera até 200 s, fora da thread da tela).
- Assuntos que ficam vazios depois do aceite continuam na sprint (não aparecem na apresentação) até serem apagados à mão.
