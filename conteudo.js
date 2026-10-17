/*
 * Conteúdo do site da Colmeia. Todo texto fica aqui; a pagina.html só monta.
 * Capturas em img/ têm 1440 x 900 e dados fictícios.
 * foco [x, y, zoom]: no cartão aparece um recorte em volta de x, y (0 a 1) com esse zoom;
 * o toque (ou clique) abre a captura inteira.
 */
window.COLMEIA = {
  nome: 'Colmeia',
  nav: { como: 'Como funciona', recursos: 'Recursos', privacidade: 'Privacidade', instalar: 'Instalar', tema: 'Alternar tema claro e escuro', menu: 'Navegação principal' },

  topo: {
    titulo: 'Um quadro de tarefas para seus agentes de IA de código',
    texto: 'A Colmeia é um app de computador que coordena Claude Code, Codex, Gemini CLI e OpenCode. Cada agente trabalha no seu terminal, as tarefas andam sozinhas pelo quadro e uma abelha avisa quando é a sua vez.',
    acao: 'Instalar',
    secundaria: 'Como funciona',
    nota: 'Linux e Windows · código aberto',
    balao: '3 tarefas esperam você',
    img: 'quadro.webp', imgClaro: 'tema-claro.webp', foco: [0.62, 0.32, 1.9],
    alt: 'O quadro de um projeto com as colunas Backlog, Agente trabalhando, Aguardando você, Revisão e Concluído. A coluna Aguardando você tem três tarefas.'
  },

  problema: {
    titulo: 'O que a Colmeia faz',
    sub: 'Tira os agentes das abas soltas.',
    texto: 'Com vários agentes rodando ao mesmo tempo, o trabalho se espalha em terminais, e é fácil perder quem parou esperando resposta. A Colmeia põe cada tarefa num quadro, com os agentes e o terminal de cada um, e mostra o que precisa de você.',
    sem: { titulo: 'Sem a Colmeia', itens: [
      'Um terminal por agente, espalhado em abas',
      'Você confere de tempos em tempos se algum parou',
      'Na reunião, você tenta lembrar o que cada agente fez',
      'O agente consulta o banco sem você ver antes'
    ] },
    com: { titulo: 'Com a Colmeia', itens: [
      'Cada tarefa no quadro, com seus agentes e terminais',
      'O cartão muda de coluna sozinho e a abelha avisa a sua vez',
      'Cada tarefa revisada deixa uma frase pronta para a daily',
      'O agente pede a consulta e você aprova antes'
    ] }
  },

  passos: {
    titulo: 'Como funciona',
    sub: 'Três passos, do pedido à revisão.',
    itens: [
      { titulo: 'Crie a tarefa', texto: 'No quadro do projeto, descreva o que precisa. A tarefa ganha uma pasta e uma branch próprias, sem misturar com o resto.', img: 'quadro.webp', foco: [0.93, 0.1, 2.6], alt: 'O botão Nova tarefa no topo do quadro.' },
      { titulo: 'Escolha os agentes', texto: 'Um ou vários: Claude Code, Codex, Gemini CLI ou OpenCode. Cada um roda no seu terminal, dentro da tarefa.', img: 'tarefa-terminal.webp', foco: [0.45, 0.24, 2.4], alt: 'Duas fichas de Claude Code, dev e testador, em cima do terminal da tarefa.' },
      { titulo: 'Acompanhe pelo quadro', texto: 'O cartão vai de Trabalhando para Aguardando você e depois para Revisão. Quando um agente para e espera você, a abelha avisa.', img: 'quadro.webp', foco: [0.63, 0.42, 2.2], alt: 'A coluna Aguardando você, em âmbar, com três tarefas.' }
    ]
  },

  recursos: {
    titulo: 'Recursos',
    sub: 'O que vem na Colmeia.',
    itens: [
      { id: 'quadro', tam: 'grande', tom: 'escuro', titulo: 'Quadro que se atualiza sozinho',
        texto: 'Backlog, Trabalhando, Aguardando você, Revisão e Concluído. O topo resume quantas tarefas esperam você, e o Histórico guarda as concluídas por dia, com busca.',
        img: 'historico.webp', foco: [0.5, 0.28, 1.45], alt: 'A aba Histórico: tarefas concluídas de ontem e, ao lado, o que aconteceu em uma delas.' },
      { id: 'abas', tam: 'grande', tom: 'claro', titulo: 'A tarefa em abas',
        texto: 'Terminal com uma ficha por agente, Arquivos com busca e diff contra a branch base, Lousa para notas e Navegador com o Play para rodar o projeto.',
        img: 'tarefa-arquivos.webp', foco: [0.63, 0.46, 1.8], alt: 'A aba Arquivos com o diff de filtros.ts: linhas removidas em vermelho e adicionadas em verde.' },
      { id: 'daily', tam: 'medio', tom: 'claro', titulo: 'Daily e sprint prontas',
        texto: 'Quando a tarefa vai para Revisão, o agente deixa uma frase sem termos técnicos. A daily junta Ontem, Hoje e Esperando, e Gerar sprint com IA propõe títulos e assuntos que você aceita ou descarta.',
        img: 'sprint-gerar.webp', foco: [0.55, 0.42, 1.8], alt: 'A proposta da sprint gerada: como está à esquerda, os títulos novos à direita, com Aceitar e Descartar.' },
      { id: 'bancos', tam: 'medio', tom: 'escuro', titulo: 'Bancos com aprovação',
        texto: 'PostgreSQL, MySQL, SQLite e SQL Server, só leitura por padrão. O agente pede a consulta, você vê o resultado e só então aprova.',
        img: 'banco-pedido.webp', foco: [0.7, 0.4, 1.9], alt: 'O pedido do Claude Code para consultar loja-dev, com a instrução travada e os botões Recusar e Aprovar e enviar.' },
      { id: 'uso', tam: 'pequeno', tom: 'claro', titulo: 'Uso do plano',
        texto: 'O pé da barra lateral mostra quanto da janela de 5 horas e da semana já foi, e o anel de cada agente mostra o contexto.',
        medidores: [{ rotulo: '5 h', valor: 34 }, { rotulo: 'Semana', valor: 61 }, { rotulo: 'Contexto', valor: 48 }] },
      { id: 'fixa', tam: 'pequeno', tom: 'escuro', titulo: 'Abelha fixa',
        texto: 'Uma abelha pequena no canto da tela, mesmo com a Colmeia fechada: âmbar quando alguém espera, verde quando trabalham.',
        img: 'abelha-fixa.webp', foco: [0.83, 0.72, 2.4], alt: 'A abelha fixa no canto da tela com o resumo aberto: 3 esperam você.' },
      { id: 'voz', tam: 'pequeno', tom: 'claro', titulo: 'Voz, se quiser',
        texto: 'Ditado, comandos falados e a voz da abelha rodam no seu computador. Tudo vem desligado.',
        img: 'voz-comandos.webp', foco: [0.45, 0.36, 2.2], alt: 'O diálogo de comandos falados e voz da abelha.' },
      { id: 'jeito', tam: 'largo', tom: 'claro', titulo: 'Temas e mascotes',
        texto: 'Quatro temas e cinco abelhas, com o nome que você escolher para os avisos.',
        temas: [
          { nome: 'Claro', img: 'tema-claro.webp' }, { nome: 'Escuro', img: 'quadro.webp' },
          { nome: 'Leitura', img: 'tema-leitura.webp' }, { nome: 'Fosco', img: 'tema-fosco.webp' }
        ],
        mascotes: [
          { nome: 'Colmeia', nota: 'padrão', img: 'mascote-colmeia.webp' },
          { nome: 'Clássica', img: 'mascote-classica.webp' },
          { nome: 'Redonda', img: 'mascote-redonda.webp' },
          { nome: 'Favo', img: 'mascote-favo.webp' },
          { nome: 'Zangão', img: 'mascote-zangao.webp' }
        ] }
    ]
  },

  leve: {
    titulo: 'Leve, local e opcional',
    sub: 'Feita para ficar aberta o dia inteiro.',
    itens: [
      { titulo: 'Sem porta de rede', texto: 'O núcleo escuta num socket local, com um token por tarefa.' },
      { titulo: 'Parada não gasta', texto: 'Tela parada não redesenha; a abelha fixa usa 0,03% da CPU.' },
      { titulo: 'Desligado custa zero', texto: 'Voz e IA extra só carregam se você ligar.' },
      { titulo: 'Seus dados ficam aqui', texto: 'Senhas no chaveiro do sistema e áudio só na memória.' }
    ]
  },

  instalar: {
    titulo: 'Instalar',
    texto: 'Baixe o pacote do seu sistema na página de versões do GitHub, confira e rode o instalador. Não precisa de permissão de administrador.',
    requisito: 'Você precisa ter pelo menos um agente instalado: Claude Code, Codex, Gemini CLI ou OpenCode.',
    sistemas: [
      { id: 'linux', nome: 'Linux', detalhe: 'x86_64', arquivo: 'colmeia-linux-x86_64.tar.gz',
        passos: [
          { titulo: 'Baixe', texto: 'O pacote e o SHA256SUMS da última versão.' },
          { titulo: 'Confira', texto: 'O SHA256SUMS garante que o arquivo é o publicado.', cmd: 'sha256sum -c SHA256SUMS --ignore-missing' },
          { titulo: 'Instale', texto: 'Vai para ~/.local/bin, com atalho e ícone no menu.', cmd: 'mkdir colmeia && tar -xzf colmeia-linux-x86_64.tar.gz -C colmeia\n./colmeia/instalar.sh' }
        ] },
      { id: 'windows', nome: 'Windows', detalhe: '10 (1803 ou mais novo) e 11', arquivo: 'colmeia-windows-x86_64.zip',
        passos: [
          { titulo: 'Baixe', texto: 'O .zip e o SHA256SUMS da última versão.' },
          { titulo: 'Extraia', texto: 'Abra o PowerShell dentro da pasta extraída.' },
          { titulo: 'Instale', texto: 'Vai para a pasta de programas do usuário, com atalho no menu Iniciar.', cmd: 'powershell -ExecutionPolicy Bypass -File .\\instalar.ps1' }
        ] }
    ],
    baixar: 'Baixar para',
    versoes: 'Todas as versões',
    compilar: 'Compilar do código',
    copiar: 'Copiar', copiado: 'Copiado', selecionado: 'Selecionado. Use Ctrl+C para copiar.'
  },

  links: {
    repositorio: 'https://github.com/AnaMarcelaSilva/colmeia',
    versoes: 'https://github.com/AnaMarcelaSilva/colmeia/releases/latest',
    baixar: 'https://github.com/AnaMarcelaSilva/colmeia/releases/latest/download/',
    compilar: 'https://github.com/AnaMarcelaSilva/colmeia#desenvolvimento',
    licenca: 'https://github.com/AnaMarcelaSilva/colmeia#licença',
    github: 'https://github.com/AnaMarcelaSilva',
    linkedin: 'https://www.linkedin.com/in/ana-marcela-rodrigues/'
  },

  ampliar: { abrir: 'Ampliar', fechar: 'Fechar', mais: 'Aproximar', menos: 'Afastar', dica: 'Arraste para mover. Toque duas vezes para aproximar.' },
  rodape: {
    sobre: 'Um quadro de tarefas para coordenar agentes de IA de código, no seu computador.',
    produto: 'Produto', projeto: 'Projeto', contato: 'Contato',
    repositorio: 'Código no GitHub', versoes: 'Versões', licenca: 'Licença',
    email: 'anamarcelarodriguessilva@gmail.com', linkedin: 'LinkedIn', github: 'GitHub',
    autora: 'Feito por Ana Marcela',
    direitos: 'Código aberto sob MIT ou Apache 2.0.',
    nota: 'As telas deste site são capturas da Colmeia com dados fictícios.'
  },
  pular: 'Pular para o conteúdo'
};
