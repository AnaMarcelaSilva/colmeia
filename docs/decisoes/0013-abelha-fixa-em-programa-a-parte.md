# 0013 · Abelha fixa num programa à parte

**Decidido em 16/10/2026.**

## Contexto

A abelha da barra lateral só aparece com a janela da Colmeia aberta e na frente. Quem deixa os agentes trabalhando e vai para o editor, o navegador ou outra área de trabalho perde o aviso de que alguém espera uma resposta. O núcleo já continua rodando com a janela fechada; faltava algo pequeno, sempre à vista, que não pesasse.

## Decisão

- **Um programa à parte, `colmeia-abelha`,** no mesmo workspace, reaproveitando o crate `mascote` (o desenho) e o `canal` (o socket e o token, que saíram da tela para um crate próprio). A janela da Colmeia liga e desliga pela opção "Fixar a abelha na área de trabalho", guardada no perfil (`abelha_fixa`). Desligar só grava: a abelha recebe o evento `perfil.abelha_fixa` e sai sozinha, e o Desafixar dela faz o mesmo caminho ao contrário. A janela abre a abelha de novo ao entrar num perfil com ela fixa; uma trava no diretório do canal deixa uma só por perfil.
- **Só eventos.** A abelha se liga ao WebSocket de eventos do perfil com `?quem=abelha` e mantém um retrato pequeno do quadro (tarefas, agentes, projetos), atualizado pelos mesmos eventos da tela. Parada, não redesenha: anima por 2,5 s depois de mudar de estado ou de o mouse chegar, e o balão anda o fio a 10 quadros por segundo enquanto está na tela.
- **Abrir a tarefa sem segundo app.** "Abrir" e "Ver o erro" pedem ao núcleo (`POST /v1/perfis/{id}/abrir`) que avise as janelas abertas do perfil (o evento `tela.abrir`, fora da corrente). A resposta diz quantas receberam, sem contar a abelha; com nenhuma, a abelha abre `colmeia --perfil N --tarefa M`, que entra direto no perfil e abre a tarefa.
- **Por cima e em todas as áreas de trabalho:** o "por cima" vem do egui; no X11, a abelha pede ao gerenciador de janelas `_NET_WM_STATE_STICKY` (e ABOVE, fora da barra de tarefas e do paginador) e `_NET_WM_DESKTOP` = todas, como manda o EWMH, pelo x11rb. Sem compositor, a janela fica opaca, no fundo escuro dos cartões. No Wayland não há como pedir isso: ela é uma janela comum, por cima só onde o compositor deixar.

## Alternativas

- **Uma segunda janela (viewport) do próprio app:** morre com a janela principal, que é justamente o que se quer fechar, e pesa o processo inteiro da tela.
- **Ícone na bandeja do sistema:** não existe em todos os ambientes (o GNOME tirou) e não mostra a personagem nem o balão.
- **O núcleo desenhar a abelha:** o núcleo não tem interface e não deve ganhar uma.

## Consequências

- Um binário a mais no instalador e na release (Linux e Windows). No Windows a abelha fica por cima, mas só na área de trabalho atual.
- Com a janela aberta em outro perfil, "Abrir" abre uma segunda janela da Colmeia no perfil da abelha.
- O erro que a abelha conta é o que chegou com ela ligada e você ainda não abriu (os de antes contam como vistos, como na janela).
