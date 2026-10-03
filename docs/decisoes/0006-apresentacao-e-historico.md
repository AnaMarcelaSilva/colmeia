# 0006 · Histórico de mensagens fora dos eventos, vídeo no reprodutor do sistema e foto recodificada

**Decidido em 02/10/2026.**

## Contexto

A entrega D trouxe três coisas que guardam conteúdo seu: o histórico do que você manda aos agentes (a seta para cima da caixa de mensagem), as notas de cada tarefa na daily e na sprint, e as fotos e vídeos anexados no modo apresentação. A tabela `eventos` é só de acréscimo e encadeada por hash: o que entra nela não sai mais.

## Decisão

- **Histórico de mensagens numa tabela própria**, `mensagens`, fora da corrente de eventos e sem evento nenhum. Um segredo colado por engano na caixa precisa poder ser apagado: o menu do agente tem "Limpar histórico de mensagens" e a API tem `DELETE /v1/agentes/{id}/mensagens`. São no máximo 200 mensagens por agente; a repetição seguida não entra; e o núcleo não guarda o que parece senha, token ou chave (`internal/segredos`: prefixos conhecidos como `sk-`, `ghp_`, `AKIA`, chave privada, JWT e `senha:`/`password=`). O filtro é uma leitura, não uma garantia, por isso o resto (limite, apagar, banco `0600`) continua valendo. O envio ao agente não muda: ele vai pelo terminal, e o histórico é só conveniência.
- **Notas numa tabela própria**, `notas`, por tarefa, tipo (daily ou sprint) e período. O evento `nota.atualizada` leva só o tamanho do texto, para a linha do tempo dizer "anotou na daily" sem prender o conteúdo no hash. A nota passa pelo mesmo filtro de segredos das mensagens: a que parece ter senha ou chave é recusada (400) e a tela diz "Não salvei: …", porque a nota vai para o telão e para o texto da daily.
- **Vídeo no reprodutor do sistema.** A tela não decodifica vídeo: mostra um cartão com o play, o nome e o tamanho, e o clique chama `xdg-open` com o caminho, sem shell e com um argumento só. O núcleo confere os primeiros bytes (`ftyp` para mp4 e mov, o cabeçalho EBML para webm e mkv), a extensão vem de uma lista fixa e os dois precisam bater: o `xdg-open` nunca recebe outra coisa com nome de vídeo (um `.desktop`, por exemplo). O vídeo é copiado em fluxo para um temporário `0600`, com o sha256 calculado durante a cópia, até 512 MB.
- **Foto JPEG vira PNG.** O núcleo confere as dimensões antes de decodificar (os mesmos limites do PNG), reduz fotos acima de 3840 px e grava um PNG novo. Recodificar descarta o EXIF, com o GPS e o modelo da câmera, e a tela continua lendo um formato só.

## Alternativas

- **Histórico como eventos:** de graça na linha do tempo, mas um segredo ficaria para sempre na corrente. Recusado.
- **Tocar o vídeo dentro do app:** exigiria um decodificador (ffmpeg ou GStreamer) e uma superfície de ataque bem maior. Fica fora; a miniatura do vídeo também, pelo mesmo motivo.
- **Um quadro do vídeo como miniatura, tirado pelo núcleo com o ffmpeg quando houver:** o pedido falava em "miniatura". Ficou de fora por ora: o núcleo passaria a rodar um decodificador sobre um arquivo que veio de fora, e o resultado dependeria do ffmpeg instalado. É uma limitação conhecida: o vídeo aparece como um cartão 16:9 com o play, o nome e o tamanho, e abre no reprodutor do sistema.
- **Guardar o JPEG como veio:** menor no disco, mas levaria o EXIF junto e a tela precisaria de um decodificador de JPEG.

## Consequências

- O histórico de um agente some junto com ele (`ON DELETE CASCADE`), e o envio para "todos os agentes" grava no histórico de cada um.
- Um PNG de foto é maior que o JPEG original; com a redução para 3840 px, uma foto de celular fica em poucos MB.
- Numa área de trabalho com D-Bus (GNOME, KDE), o `xdg-open` pede o reprodutor à sessão: ele abre na tela da sessão, que é a da Colmeia no uso normal. Num teste em outra tela (Xephyr), use um `xdg-open` falso no `PATH`.
- Com a apresentação em tela cheia, o gerenciador de janelas pode abrir o reprodutor atrás dela; nesse caso, F11 sai da tela cheia.
