# 0018 · A abelha fixa como "dock" e escondida com a janela à vista

**Decidido em 17/10/2026.**

## Contexto

A abelha fixa ([0013](0013-abelha-fixa-em-programa-a-parte.md)) ficava por cima da janela da Colmeia e cobria os botões do topo; o "mostrar a área de trabalho" (Super+D) do GNOME a escondia; e dois cliques seguidos davam um salto (a posição que o gerenciador ainda não tinha aplicado era lida como arraste).

## Decisão

- **Tipo "dock" no X11** (`_NET_WM_WINDOW_TYPE_DOCK`, posto antes de a janela aparecer), com ABOVE, STICKY, SKIP_TASKBAR e SKIP_PAGER como antes. O "mostrar a área de trabalho" não minimiza docks, e elas não vão à barra de tarefas nem ao Alt+Tab.
- **O arraste é da própria abelha:** só começa depois de 6 px com o botão apertado; enquanto isso a posição vem do ponteiro na tela (pelo X11, que continua respondendo com o ponteiro fora da janela) e a janela vai junto. O gerenciador não move docks, e um clique (mesmo duplo) não mexe em nada.
- **Escondida com a janela à vista.** A janela da Colmeia se apresenta com um nome nos eventos (`?janela=`) e diz ao núcleo quando minimiza ou volta (`POST /v1/perfis/{id}/janela`, só quando muda). O núcleo conta as janelas abertas e não minimizadas e avisa só a abelha (`tela.janelas`, sem número de sequência, e no olá dela). Com alguma à vista, a abelha some e volta no mesmo lugar; falando com ela (ou com a resposta no balão), fica. Os avisos seguem na janela.
- **O microfone continua sempre à vista:** com a abelha fixa escondida e o "ouvir o nome" ligado, a abelha da barra lateral põe o fone.

## Alternativas

- **Consultar as janelas de tempos em tempos:** contra a regra de tudo por evento.
- **Mover a abelha para fora do caminho:** ela ainda cobriria alguma coisa; esconder é previsível.

## Consequências

- No Xephyr sem gerenciador de janelas: o tipo DOCK confere no `xprop`, o arraste leva a abelha até o ponteiro e grava o lugar, dois cliques não a movem, e ela some quando uma janela entra no perfil e volta ao fechar. O efeito do Super+D e a ordem por cima no Mutter só dá para confirmar na máquina real (o Mutter não roda dentro do Xephyr sem disputar com a sessão).
- Num gerenciador que não deixe uma dock receber o teclado, a caixa de escrever à abelha pode não pegar o foco; os comandos falados e os botões não dependem disso.
- Minimizar sem gerenciador de janelas não existe no Xephyr: o caminho de minimizar tem teste no núcleo e foi conferido só pela API.
