# Mudanças

O formato segue [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/) e as versões seguem [SemVer](https://semver.org/lang/pt-BR/).

## [0.2.0] · 02/10/2026

### Novo

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

### Mudou

- O núcleo grava o perfil, o projeto, a tarefa e o agente de cada evento (dentro do hash e em colunas indexadas) e o título nas mudanças de tarefa. Os eventos antigos são ligados aos perfis na primeira abertura.
- A regra "agente começou numa tarefa do Backlog: vai para Trabalhando" saiu da tela e foi para o núcleo.
- Imagens coladas e capturas são guardadas pelo núcleo (decodificadas e codificadas de novo, sem metadados, `0600`).
- O terminal da tela dorme no `poll(2)` em vez de acordar 200 vezes por segundo: com 10 agentes parados, a tela parada foi de 9% para 0% de processador.
- Os temas claro e leitura passaram a ter os mesmos tamanhos e espaçamentos do escuro; as cores de estado ganharam contraste.
- `scripts/medir.sh --pid` mede um processo exato.

### Corrigido

- Um núcleo iniciado pela tela que caía ficava como processo zumbi até a tela fechar.

## [0.1.0] · 02/10/2026

- Perfis, contas de IA, workspaces, projetos (git ou pasta), tarefas e agentes de verdade, cópias isoladas por git worktree, retomar conversa do Claude Code, teclado completo no terminal.
