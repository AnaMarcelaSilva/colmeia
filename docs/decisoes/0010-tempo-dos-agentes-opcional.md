# 0010 · Tempo dos agentes só quando pedido, escondido pelo núcleo

**Decidido em 05/10/2026.**

## Contexto

A daily, a sprint, a linha do tempo e a apresentação mostravam quanto cada agente trabalhou e quanto esperou você ("Claude Code (dev) trabalhou 1h05 e esperou você 12 min", "Agentes trabalharam 2h", o tempo por ferramenta). A usuária apresenta essas telas e cola o texto em reuniões, e disse que é uma informação sensível: quer ver só quando escolher.

Os tempos nascem num ponto só, quando a sessão do agente termina (`agente.terminou` grava `trabalhando_s` e `aguardando_s` na corrente), e daí viram somas e frases no pacote `linha` do núcleo. A tela só mostra o que o núcleo monta.

## Decisão

- **Desligado por padrão, por perfil.** A opção "Mostrar tempo dos agentes" é a coluna `perfis.tempo_agentes` (0 por padrão), mudada por `PATCH /v1/perfis/{id}` com `tempo_agentes`. Fica no núcleo, não na tela: vale em qualquer janela e sobrevive a fechar a Colmeia.
- **O núcleo nem manda os tempos.** Com a opção desligada, a montagem zera a duração e a espera de cada sessão logo na origem: as somas por dia, projeto e ferramenta ficam em zero, os campos `tempo_s`, `tempo_total_s` e `por_ferramenta` saem do JSON, e os textos dizem só "Claude Code (dev) terminou uma sessão" ("Claude Code (dev): 3 sessões" no slide). A sessão aberta perde o "desde 14:26", que diz quanto ele espera. O Markdown da sprint e o texto da daily saem sem tempo. Cada resposta diz `tempo_agentes`, para a tela saber o que veio.
- **Os dados continuam gravados.** A corrente de eventos é só acréscimo (decisão 0004): `agente.terminou` mantém os tempos, e ligar a opção mostra o passado também.
- **A troca chega às outras telas por evento.** `perfil.tempo_agentes` vai pelo WebSocket só com o booleano. A tela esconde na hora ao desligar (tira da memória o que veio com tempo e só deixa copiar ou apresentar depois da resposta sem ele) e, ao ligar, mantém o que está na tela e deixa o tempo aparecer quando a resposta chega. Uma resposta com tempo que chega depois de desligar é descartada.

## Alternativas

- **Esconder só na tela:** o texto copiado e o Markdown salvo vêm prontos do núcleo e levariam o tempo; outra tela (o celular, no futuro) teria de repetir a regra. O núcleo esconder é uma regra só.
- **Guardar a opção no navegador da tela (arquivo local):** valeria só naquela máquina e naquela janela; a apresentação aberta em outra janela mostraria o tempo.
- **Não gravar os tempos:** perderia o dado para sempre; a usuária quer poder ligar quando quiser.

## Consequências

- O cartão do agente no quadro e os eventos da corrente lidos direto do banco ainda têm a duração; o cartão não mostra a duração hoje, e esconder também ali fica para depois, se fizer falta.
- Uma apresentação já aberta quando a opção muda em outra janela esconde os números de tempo na hora, mas os textos do deck só mudam ao atualizar (R).
