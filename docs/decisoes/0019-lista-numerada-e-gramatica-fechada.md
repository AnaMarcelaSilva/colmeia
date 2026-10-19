# 0019 · A lista numerada e a gramática fechada

**Decidido em 17/10/2026.**

## Contexto

No teste com o microfone de verdade, o ditado ([0014](0014-ditado-em-processo-a-parte.md)) transcreveu muito bem, mas os comandos falados ([0015](0015-comandos-falados-e-voz-da-abelha.md), [0017](0017-voz-paciente-e-precisa.md)) falharam: eles dependem de o Whisper escrever certo o nome de uma tarefa ou projeto e de a busca aproximada achar o nome. Com títulos longos, parecidos ou em inglês, isso erra muito. A usuária propôs que a abelha diga o que importa com números e que o comando seja só um verbo e um número.

Também apareceu que, com a janela da Colmeia minimizada, a abelha fixa não voltava (só ao fechar a janela).

## Decisão

- **Chamar a abelha mostra a lista.** O clique na abelha (a da barra lateral ou a fixa), o atalho e o nome ouvido pedem ao núcleo `POST /v1/perfis/{id}/voz/lista`. O núcleo (`comandovoz.MontarLista`) junta, uma vez por tarefa e fora das concluídas, o que espera você (agente aguardando ou tarefa em "Aguardando você"), o que tem agente trabalhando e o que tem agente parado com erro, nessa ordem e, em cada grupo, da tarefa mais antiga para a mais nova (pelo número da tarefa, que não muda). A página tem 5 itens, numerados de 1 a 5 em cada página ("1. teste: o Claude Code espera você."); o título longo fica nas primeiras palavras. A abelha mostra no balão e, com a voz ligada, fala. Quando a fala acaba (ou na hora, sem a voz), o microfone abre para o comando.
- **A lista fica no núcleo, presa ao perfil.** Uma ficha aleatória, válida por 2 minutos desde o último uso, guarda os itens como foram falados: o número vale para o que a pessoa ouviu, mesmo que o quadro mude no meio. A ficha vai junto na frase (`"lista"` no `POST …/voz/comando`); de outro perfil ou vencida, não vale. Uma lista nova troca a anterior; feita a ação, ela fecha.
- **Gramática fechada, a frase mais parecida.** Valem: abrir, responder, concluir, enviar para revisão, parar e rodar com o número (1 a 9, por extenso, ordinal ou algarismo: "um", "uma", "primeira", "2", "segunda"), o número sozinho (abre), próxima, anterior, repetir e fechar/cancelar; com uma pergunta no ar, sim e não. Só os números da página entram. A frase transcrita, sem o nome da abelha, as cortesias e as palavras que não mudam o pedido ("a", "o", "tarefa", "número"), é comparada com todas as frases válidas por três leituras (a escrita, sem espaços e um "som" simplificado do português: "hum" e "um", "treis" e "três") e vale a mais parecida, com nota mínima de 0,72 e folga para a melhor de outro sentido (0,08 na nota ou o erro dela 2,5 vezes maior). O Whisper ouve com um prompt só dessas frases. Sem confiança, a abelha pergunta de novo uma vez ("Não entendi. Diga, por exemplo: abrir a um."), e diz até onde vai a lista quando o número dito não existe; na segunda vez, a lista fecha.
- **O que cada verbo faz.** Abrir abre a tarefa. Responder abre a tarefa no agente que espera você (ou no primeiro rodando) e liga o ditado na caixa dele (a janela recebe `ditar` no `tela.abrir`, ou `--ditar` quando a abelha fixa abre a Colmeia; com o ditado desligado no perfil, só abre). Concluir pede o "sim", como encerrar. Enviar para revisão move na hora. Rodar e parar valem para o projeto da tarefa, com as mesmas regras de antes (parar sempre pergunta; com mais de uma configuração, a abelha diz quais e espera o nome). Toda pergunta reabre o microfone com um prompt de sim e não.
- **Os comandos por nome continuam.** Com a lista aberta, uma frase que não é da lista mas é um comando por nome ("rodar a loja web") segue o caminho de antes e fecha a lista, menos quando o nome dito parece um número ("abrir a hum" não procura uma tarefa chamada "hum"). Uma frase que não lembra a lista (nota abaixo de 0,55) vai para a pergunta livre, se ligada. Só o nome da abelha, dito com o microfone aberto, abre a lista.
- **Ouvir o nome chama a lista.** Ouvido o nome, a abelha fixa cancela o comando que começava a ouvir e pede a lista; o microfone abre depois da fala dela.
- **A janela diz quando sai de vista, pelo X11.** Minimizar no X11 não dá quadro ao egui, e trocar de área de trabalho nem muda o estado dele: a janela não avisava o núcleo. Agora uma thread da janela, com a própria conexão ao X11, escuta por eventos (sem consulta) o mapear e desmapear dela, o `_NET_WM_STATE` (`HIDDEN`), o `_NET_WM_DESKTOP` dela e o `_NET_CURRENT_DESKTOP` da raiz, e chama o mesmo `POST /v1/perfis/{id}/janela` quando ela some ou volta. Fora do X11 continua o que o egui informa.

## Alternativas

- **Melhorar a busca de nomes:** os nomes são abertos (qualquer título) e o Whisper erra justamente neles; a lista troca o nome por um número, que tem poucas formas.
- **Regras (regex) para "verbo + número":** quebram com a transcrição imperfeita ("abriram", "responderá um"); comparar com a lista fechada das frases válidas tolera.
- **Numerar a lista inteira (6, 7, 8…):** números maiores soam parecidos e a fala fica longa; numerar por página mantém tudo de 1 a 5.
- **A abelha fixa olhar as janelas da Colmeia no X11:** precisaria achar as janelas do mesmo perfil e da mesma instância; a própria janela saber de si é mais simples e vale para qualquer perfil.

## Consequências

- O clique na abelha agora fala antes de ouvir: o caminho padrão leva alguns segundos a mais (a fala da lista), e o comando direto continua pelo nome depois dela.
- Com a lista aberta o Whisper ouve com o prompt da gramática, então um comando por nome dito nessa hora pode sair pior escrito que antes.
- O núcleo guarda, só na memória, a última lista de cada perfil por até 2 minutos.
