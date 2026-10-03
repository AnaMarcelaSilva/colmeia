// Package linha transforma o histórico de eventos na linha do tempo e nos
// resumos de daily e sprint. Tudo aqui é cálculo sem efeito colateral: recebe
// os eventos, o "agora" e o fuso, e devolve textos em português prático,
// prontos para falar. Os textos ficam no núcleo para qualquer tela (a desktop,
// o celular no futuro) mostrar o mesmo.
package linha

import (
	"encoding/json"
	"fmt"
	"sort"
	"strings"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/ferramentas"
)

// Movimentos da mesma tarefa mais próximos que isso viram uma linha só.
const juntarMovimentos = 10 * time.Minute

// Contexto é o que os eventos sozinhos não dizem: o momento e o fuso, os nomes
// atuais (para eventos antigos, gravados sem título) e o que existe agora.
type Contexto struct {
	Agora time.Time
	Fuso  *time.Location
	// Projetos e tarefas que existem agora, pelo id.
	Projetos map[int64]string
	Tarefas  map[int64]TarefaAtual
	// Agentes rodando agora, com o estado de cada um: uma sessão sem fim só
	// aparece como aberta se ainda roda.
	Ativos          map[int64]Ativo
	AnexosRemovidos map[int64]bool
	// VariosProjetos: o escopo é o perfil e há mais de um projeto, então o
	// nome do projeto aparece junto da tarefa nos textos.
	VariosProjetos bool
	// NomesNoEscopo são os projetos olhados, para o cabeçalho dos resumos.
	NomesNoEscopo []string
}

// Ativo é um agente rodando agora: o estado e desde quando (a última
// mudança, a mesma hora que o cartão mostra).
type Ativo struct {
	Estado string
	Desde  time.Time
	Tarefa int64
}

type TarefaAtual struct {
	ID        int64
	Titulo    string
	Coluna    string
	ProjetoID int64
}

// Item é uma linha da linha do tempo.
type Item struct {
	Evento    int64   `json:"evento"`
	Momento   string  `json:"momento"`
	Hora      string  `json:"hora"`
	Tipo      string  `json:"tipo"`
	Texto     string  `json:"texto"`
	ProjetoID int64   `json:"projeto_id,omitempty"`
	Projeto   string  `json:"projeto,omitempty"`
	TarefaID  int64   `json:"tarefa_id,omitempty"`
	AgenteID  int64   `json:"agente_id,omitempty"`
	Removida  bool    `json:"removida,omitempty"`
	Anexos    []int64 `json:"anexos,omitempty"`
	// Videos são os anexos (de Anexos) que são vídeos: a tela não pede imagem deles.
	Videos []int64 `json:"videos,omitempty"`
	// Curto é o texto sem o nome da tarefa, para quando ela já aparece em
	// volta (o cartão da tarefa na linha do tempo, o slide da apresentação).
	Curto string `json:"curto,omitempty"`
	// Titulo e Coluna da tarefa agora (a coluna fica vazia se ela saiu).
	Titulo string `json:"titulo,omitempty"`
	Coluna string `json:"coluna,omitempty"`

	quando      time.Time
	titulo      string
	coluna      string // coluna final de um movimento
	ferramenta  string
	trabalhando int64
	// Sessão que terminou bem: o agente ("Claude Code (dev)") e o tempo que
	// esperou você, para a apresentação juntar as sessões numa linha.
	agente     string
	aguardando int64
	erro       bool
	descartado bool
}

// Tipos de item (a tela escolhe o ponto pela tabela de estados).
const (
	TipoConcluiu     = "concluiu"
	TipoCriou        = "criou"
	TipoMoveu        = "moveu"
	TipoRemoveu      = "removeu"
	TipoSessao       = "sessao"
	TipoSessaoAberta = "sessao_aberta"
	// Sessão aberta de um agente que espera você agora.
	TipoSessaoAguardando = "sessao_aguardando"
	// Sessão aberta de um terminal parado: não é trabalho acontecendo.
	TipoSessaoParada = "sessao_parada"
	TipoErro         = "erro"
	TipoInterrompido = "interrompido"
	TipoCaptura      = "captura"
	TipoProjeto      = "projeto"
	TipoNota         = "nota"
)

type Dia struct {
	Dia    string `json:"dia"` // AAAA-MM-DD, no fuso
	Titulo string `json:"titulo"`
	Resumo string `json:"resumo"`
	Itens  []Item `json:"itens"`
	// Números do dia, para o cabeçalho (o que veio nesta página).
	Concluidas int   `json:"concluidas"`
	Erros      int   `json:"erros"`
	TempoS     int64 `json:"tempo_s"`
}

var nomesColuna = map[string]string{
	"backlog": "Backlog", "trabalhando": "Agente trabalhando", "aguardando": "Aguardando você", "revisao": "Revisão", "concluido": "Concluído",
}

var diasSemana = [...]string{"domingo", "segunda", "terça", "quarta", "quinta", "sexta", "sábado"}
var meses = [...]string{"janeiro", "fevereiro", "março", "abril", "maio", "junho", "julho", "agosto", "setembro", "outubro", "novembro", "dezembro"}

// conteudo junta os campos que os eventos usam (gravados por versões
// diferentes: "tarefa" é um número nos antigos e o objeto inteiro nos novos).
type conteudo struct {
	Tarefa       json.RawMessage `json:"tarefa"`
	ID           int64           `json:"id"`
	Titulo       string          `json:"titulo"`
	Nome         string          `json:"nome"`
	ProjetoID    int64           `json:"projeto_id"`
	ProjetoNome  string          `json:"projeto_nome"`
	Coluna       string          `json:"coluna"`
	ColunaAntes  string          `json:"coluna_antes"`
	Origem       string          `json:"origem"`
	Mudanca      struct{ Titulo, Coluna *string }
	Agente       int64  `json:"agente"`
	Ferramenta   string `json:"ferramenta"`
	Papel        string `json:"papel"`
	Motivo       string `json:"motivo"`
	Codigo       int    `json:"codigo"`
	Erro         bool   `json:"erro"`
	TrabalhandoS int64  `json:"trabalhando_s"`
	AguardandoS  int64  `json:"aguardando_s"`
	Anexo        int64  `json:"anexo"`
	Tipo         string `json:"tipo"`
	Periodo      string `json:"periodo"`
	Tamanho      int    `json:"tamanho"`
}

type tarefaNoEvento struct {
	ID        int64  `json:"id"`
	Titulo    string `json:"titulo"`
	Coluna    string `json:"coluna"`
	ProjetoID int64  `json:"projeto_id"`
}

// montador acompanha os nomes enquanto os eventos passam, do mais antigo ao mais novo.
type montador struct {
	c        Contexto
	titulos  map[int64]string
	projetos map[int64]string
	itens    []Item
	movendo  map[int64]int  // tarefa → índice do último movimento
	captura  map[int64]int  // tarefa → índice da última captura
	videos   map[int64]int  // tarefa → índice do último vídeo
	notas    map[string]int // tarefa e tipo → índice da última nota
	abertas  map[int64]int  // agente → índice da sessão sem fim
	// Todas as mudanças de coluna, inclusive as automáticas (que não viram
	// item): a sprint refaz por elas onde cada tarefa estava no fim do período.
	colunas []mudancaDeColuna
}

type mudancaDeColuna struct {
	quando  time.Time
	tarefa  int64
	projeto int64
	coluna  string // "" quando a tarefa saiu
}

func novoMontador(c Contexto) *montador {
	if c.Fuso == nil {
		c.Fuso = time.Local
	}
	m := &montador{c: c, titulos: map[int64]string{}, projetos: map[int64]string{}, movendo: map[int64]int{}, captura: map[int64]int{}, abertas: map[int64]int{},
		videos: map[int64]int{}, notas: map[string]int{}}
	for id, nome := range c.Projetos {
		m.projetos[id] = nome
	}
	for id, t := range c.Tarefas {
		m.titulos[id] = t.Titulo
	}
	return m
}

func (m *montador) titulo(id int64) string {
	if t := m.titulos[id]; t != "" {
		return t
	}
	return fmt.Sprintf("tarefa #%d", id)
}

func (m *montador) nomeProjeto(id int64) string { return m.projetos[id] }

// cita é o título entre aspas. O projeto não entra: na linha do tempo ele
// já aparece na etiqueta do item (nos resumos, veja nomeNoTexto).
func (m *montador) cita(tarefa, _ int64) string {
	return "“" + m.titulo(tarefa) + "”"
}

func agente(ferramenta, papel string) string {
	nome := ferramentas.Nome(ferramenta)
	if papel != "" {
		nome += " (" + papel + ")"
	}
	return nome
}

// Duracao em português curto: "1h05", "12 min", "menos de 1 min".
func Duracao(segundos int64) string {
	switch {
	case segundos < 60:
		return "menos de 1 min"
	case segundos < 3600:
		return fmt.Sprintf("%d min", segundos/60)
	default:
		return fmt.Sprintf("%dh%02d", segundos/3600, (segundos%3600)/60)
	}
}

func (m *montador) novo(e dados.Evento, quando time.Time, tipo string) *Item {
	m.itens = append(m.itens, Item{
		Evento: e.ID, Momento: e.Momento, Tipo: tipo, quando: quando,
		ProjetoID: e.Escopo.Projeto, Projeto: m.nomeProjeto(e.Escopo.Projeto), TarefaID: e.Escopo.Tarefa, AgenteID: e.Escopo.Agente,
	})
	return &m.itens[len(m.itens)-1]
}

// passar lê um evento e cria (ou junta) o item dele.
func (m *montador) passar(e dados.Evento) {
	quando, err := time.Parse(time.RFC3339Nano, e.Momento)
	if err != nil {
		return
	}
	var d conteudo
	json.Unmarshal(e.Dados, &d)
	var t tarefaNoEvento
	if len(d.Tarefa) > 0 && d.Tarefa[0] == '{' {
		json.Unmarshal(d.Tarefa, &t)
	}
	if d.ProjetoNome != "" && e.Escopo.Projeto != 0 {
		m.projetos[e.Escopo.Projeto] = d.ProjetoNome
	}
	tarefa, projeto := e.Escopo.Tarefa, e.Escopo.Projeto

	switch e.Tipo {
	case "projeto.criado":
		m.projetos[d.ID] = d.Nome
		item := m.novo(e, quando, TipoProjeto)
		item.Projeto = d.Nome
		item.Texto = "Adicionou o projeto " + d.Nome + "."
	case "projeto.removido":
		item := m.novo(e, quando, TipoProjeto)
		item.Texto = "Tirou o projeto " + cmpOr(d.ProjetoNome, m.nomeProjeto(projeto), "sem nome") + " da Colmeia."
	case "tarefa.criada":
		m.titulos[d.ID] = d.Titulo
		m.colunas = append(m.colunas, mudancaDeColuna{quando, d.ID, projeto, "backlog"})
		item := m.novo(e, quando, TipoCriou)
		item.titulo = d.Titulo
		item.Texto = "Criou a tarefa “" + d.Titulo + "”"
		if nome := m.nomeProjeto(projeto); nome != "" {
			item.Texto += " em " + nome
		}
		item.Texto += "."
		item.Curto = "Tarefa criada."
	case "tarefa.atualizada":
		m.atualizada(e, quando, d, t)
	case "tarefa.removida":
		if d.Titulo != "" {
			m.titulos[tarefa] = d.Titulo
		}
		item := m.novo(e, quando, TipoRemoveu)
		item.Texto = "Removeu a tarefa " + m.cita(tarefa, projeto) + "."
		item.Curto = "Tarefa removida."
		delete(m.movendo, tarefa)
		m.colunas = append(m.colunas, mudancaDeColuna{quando, tarefa, projeto, ""})
	case "agente.iniciou":
		if d.Titulo != "" {
			m.titulos[tarefa] = d.Titulo
		}
		ativo, roda := m.c.Ativos[e.Escopo.Agente]
		if !roda {
			return // terminou depois (o fim conta a sessão) ou o núcleo caiu sem gravar o fim
		}
		item := m.novo(e, quando, TipoSessaoAberta)
		item.ferramenta = d.Ferramenta
		agoraFaz := " trabalhando agora em "
		switch ativo.Estado {
		case "aguardando":
			item.Tipo, agoraFaz = TipoSessaoAguardando, " esperando você em "
		case "ocioso":
			item.Tipo, agoraFaz = TipoSessaoParada, " parado em "
		}
		// "desde" é a última mudança de estado, como no cartão; sem ela, o início.
		desde := ativo.Desde
		if desde.IsZero() {
			desde = quando
		}
		item.Texto = agente(d.Ferramenta, d.Papel) + agoraFaz + m.cita(tarefa, projeto) + " desde " + desde.In(m.c.Fuso).Format("15:04") + "."
		item.Curto = agente(d.Ferramenta, d.Papel) + strings.TrimSuffix(agoraFaz, " em ") + " desde " + desde.In(m.c.Fuso).Format("15:04") + "."
		m.abertas[e.Escopo.Agente] = len(m.itens) - 1
	case "agente.terminou":
		if d.Titulo != "" {
			m.titulos[tarefa] = d.Titulo
		}
		if i, ok := m.abertas[e.Escopo.Agente]; ok {
			m.itens[i].descartado = true
			delete(m.abertas, e.Escopo.Agente)
		}
		m.terminou(e, quando, d)
	case "anexo.adicionado":
		if m.c.AnexosRemovidos[d.Anexo] {
			return
		}
		if d.Titulo != "" {
			m.titulos[tarefa] = d.Titulo
		}
		// Capturas seguidas da mesma tarefa viram uma linha com várias
		// miniaturas; vídeos seguidos, outra.
		juntas := m.captura
		if d.Tipo == "video" {
			juntas = m.videos
		}
		if i, ok := juntas[tarefa]; ok && tarefa != 0 && quando.Sub(m.itens[i].quando) <= juntarMovimentos && !m.itens[i].descartado {
			item := &m.itens[i]
			item.Anexos = append(item.Anexos, d.Anexo)
			if d.Tipo == "video" {
				item.Videos = append(item.Videos, d.Anexo)
			}
			item.quando, item.Momento, item.Evento = quando, e.Momento, e.ID
			item.Texto = textoCaptura(len(item.Anexos), d, m.citaSeHouver(tarefa, projeto))
			item.Curto = textoCaptura(len(item.Anexos), d, "")
			return
		}
		item := m.novo(e, quando, TipoCaptura)
		item.Anexos = []int64{d.Anexo}
		if d.Tipo == "video" {
			item.Videos = []int64{d.Anexo}
		}
		item.Texto = textoCaptura(1, d, m.citaSeHouver(tarefa, projeto))
		if tarefa != 0 {
			item.Curto = textoCaptura(1, d, "")
			juntas[tarefa] = len(m.itens) - 1
		}
	case "nota.atualizada":
		if d.Titulo != "" {
			m.titulos[tarefa] = d.Titulo
		}
		onde, de := "na daily", "da daily"
		if d.Tipo == "sprint" {
			onde, de = "na sprint", "da sprint"
		}
		chave := fmt.Sprintf("%d/%s", tarefa, d.Tipo)
		texto, curto := "Anotou "+onde+" sobre "+m.cita(tarefa, projeto)+".", "Anotou "+onde+"."
		if d.Tamanho == 0 {
			texto, curto = "Apagou a nota "+de+" de "+m.cita(tarefa, projeto)+".", "Apagou a nota "+de+"."
		}
		// Várias gravações seguidas (a nota salva ao perder o foco) viram uma linha.
		if i, ok := m.notas[chave]; ok && quando.Sub(m.itens[i].quando) <= juntarMovimentos && !m.itens[i].descartado {
			item := &m.itens[i]
			item.quando, item.Momento, item.Evento, item.Texto, item.Curto = quando, e.Momento, e.ID, texto, curto
			return
		}
		item := m.novo(e, quando, TipoNota)
		item.Texto, item.Curto = texto, curto
		m.notas[chave] = len(m.itens) - 1
	}
}

func textoCaptura(n int, d conteudo, cita string) string {
	var texto string
	switch {
	case d.Tipo == "video" && n > 1:
		texto = fmt.Sprintf("%d vídeos anexados", n)
	case d.Tipo == "video":
		texto = "Vídeo anexado"
	case d.Origem == "captura" && d.Ferramenta != "":
		texto = "Captura do terminal de " + agente(d.Ferramenta, d.Papel)
	case d.Origem == "captura":
		texto = "Captura do terminal"
	default:
		texto = "Imagem anexada"
	}
	if n > 1 && d.Tipo != "video" {
		texto = fmt.Sprintf("%s (%d imagens)", texto, n)
	}
	if cita != "" {
		texto += " em " + cita
	}
	return texto + "."
}

func (m *montador) citaSeHouver(tarefa, projeto int64) string {
	if tarefa == 0 {
		return ""
	}
	return m.cita(tarefa, projeto)
}

func (m *montador) atualizada(e dados.Evento, quando time.Time, d conteudo, t tarefaNoEvento) {
	tarefa, projeto := e.Escopo.Tarefa, e.Escopo.Projeto
	antes := m.titulo(tarefa)
	var colunaNova string
	if t.ID != 0 {
		m.titulos[tarefa] = t.Titulo
		if t.Coluna != d.ColunaAntes {
			colunaNova = t.Coluna
		}
	} else {
		if d.Mudanca.Titulo != nil {
			m.titulos[tarefa] = *d.Mudanca.Titulo
		}
		if d.Mudanca.Coluna != nil {
			colunaNova = *d.Mudanca.Coluna
		}
	}
	if colunaNova != "" {
		m.colunas = append(m.colunas, mudancaDeColuna{quando, tarefa, projeto, colunaNova})
	}
	// As mudanças automáticas (Trabalhando ↔ Aguardando) já aparecem nas
	// sessões dos agentes; aqui seriam só ruído.
	if d.Origem == dados.OrigemAutomatica {
		return
	}
	if colunaNova == "" {
		if novo := m.titulo(tarefa); novo != antes && d.Mudanca.Titulo != nil {
			item := m.novo(e, quando, TipoMoveu)
			item.Texto = "Renomeou “" + antes + "” para “" + novo + "”."
			item.Curto = "Renomeada de “" + antes + "”."
		}
		return
	}
	if i, ok := m.movendo[tarefa]; ok && quando.Sub(m.itens[i].quando) <= juntarMovimentos && !m.itens[i].descartado {
		item := &m.itens[i]
		item.quando, item.Momento, item.Evento, item.coluna = quando, e.Momento, e.ID, colunaNova
		m.textoMovimento(item, tarefa, projeto)
		return
	}
	item := m.novo(e, quando, TipoMoveu)
	item.coluna = colunaNova
	m.textoMovimento(item, tarefa, projeto)
	m.movendo[tarefa] = len(m.itens) - 1
}

func (m *montador) textoMovimento(item *Item, tarefa, projeto int64) {
	item.titulo = m.titulo(tarefa)
	if item.coluna == "concluido" {
		item.Tipo = TipoConcluiu
		item.Texto = "Concluiu " + m.cita(tarefa, projeto) + "."
		item.Curto = "Concluída."
		return
	}
	item.Tipo = TipoMoveu
	item.Texto = m.cita(tarefa, projeto) + " foi para " + cmpOr(nomesColuna[item.coluna], item.coluna) + "."
	item.Curto = "Foi para " + cmpOr(nomesColuna[item.coluna], item.coluna) + "."
}

func (m *montador) terminou(e dados.Evento, quando time.Time, d conteudo) {
	tarefa, projeto := e.Escopo.Tarefa, e.Escopo.Projeto
	nome := agente(d.Ferramenta, d.Papel)
	item := m.novo(e, quando, TipoSessao)
	item.ferramenta, item.trabalhando, item.titulo = d.Ferramenta, d.TrabalhandoS, m.titulo(tarefa)
	cita := m.cita(tarefa, projeto)
	switch d.Motivo {
	case "erro":
		item.Tipo, item.erro = TipoErro, true
		item.Texto = fmt.Sprintf("%s parou com erro (código %d) em %s.", nome, d.Codigo, cita)
		item.Curto = fmt.Sprintf("%s parou com erro (código %d).", nome, d.Codigo)
	case "interrompido":
		item.Tipo = TipoInterrompido
		item.Texto = nome + " foi interrompido em " + cita + "."
		item.Curto = nome + " foi interrompido."
	default:
		item.agente, item.aguardando = nome, d.AguardandoS
		item.Texto = nome + " trabalhou " + Duracao(d.TrabalhandoS) + " em " + cita
		item.Curto = nome + " trabalhou " + Duracao(d.TrabalhandoS)
		if d.AguardandoS >= 60 {
			item.Texto += " e esperou você " + Duracao(d.AguardandoS)
			item.Curto += " e esperou você " + Duracao(d.AguardandoS)
		}
		item.Texto += "."
		item.Curto += "."
	}
}

func cmpOr(v ...string) string {
	for _, s := range v {
		if s != "" {
			return s
		}
	}
	return ""
}

// itens passa os eventos (em qualquer ordem) e devolve os itens do mais antigo
// ao mais novo, já com hora, projeto e "removida" preenchidos.
func (m *montador) montar(eventos []dados.Evento) []Item {
	ordenados := append([]dados.Evento(nil), eventos...)
	sort.Slice(ordenados, func(i, j int) bool { return ordenados[i].ID < ordenados[j].ID })
	for _, e := range ordenados {
		m.passar(e)
	}
	lista := make([]Item, 0, len(m.itens))
	for _, item := range m.itens {
		if item.descartado {
			continue
		}
		item.Hora = item.quando.In(m.c.Fuso).Format("15:04")
		if item.Projeto == "" {
			item.Projeto = m.nomeProjeto(item.ProjetoID)
		}
		if item.TarefaID != 0 {
			atual, existe := m.c.Tarefas[item.TarefaID]
			item.Removida = !existe
			item.Titulo, item.Coluna = m.titulo(item.TarefaID), atual.Coluna
		}
		lista = append(lista, item)
	}
	return lista
}

func diaDe(t time.Time, fuso *time.Location) string { return t.In(fuso).Format("2006-01-02") }

// TituloDoDia: "Hoje · quinta, 1 de outubro", "Ontem", "sexta, 25 de setembro".
func TituloDoDia(dia string, agora time.Time, fuso *time.Location) string {
	d, err := time.ParseInLocation("2006-01-02", dia, fuso)
	if err != nil {
		return dia
	}
	hoje := agora.In(fuso)
	longo := fmt.Sprintf("%s, %d de %s", diasSemana[d.Weekday()], d.Day(), meses[d.Month()-1])
	if d.Year() != hoje.Year() {
		longo += fmt.Sprintf(" de %d", d.Year())
	}
	switch dia {
	case hoje.Format("2006-01-02"):
		return "Hoje · " + longo
	case hoje.AddDate(0, 0, -1).Format("2006-01-02"):
		return "Ontem · " + longo
	}
	return longo
}

func plural(n int, um, varios string) string {
	if n == 1 {
		return "1 " + um
	}
	return fmt.Sprintf("%d %s", n, varios)
}

// Montar agrupa os eventos em dias, do mais novo para o mais antigo, com os
// itens de cada dia também do mais novo para o mais antigo.
func Montar(eventos []dados.Evento, c Contexto) []Dia {
	m := novoMontador(c)
	itens := m.montar(eventos)
	dias := []Dia{}
	for i := len(itens) - 1; i >= 0; i-- {
		item := itens[i]
		dia := diaDe(item.quando, m.c.Fuso)
		if len(dias) == 0 || dias[len(dias)-1].Dia != dia {
			dias = append(dias, Dia{Dia: dia, Titulo: TituloDoDia(dia, c.Agora, m.c.Fuso)})
		}
		dias[len(dias)-1].Itens = append(dias[len(dias)-1].Itens, item)
	}
	for i := range dias {
		var concluidas, erros int
		var segundos int64
		for _, item := range dias[i].Itens {
			switch item.Tipo {
			case TipoConcluiu:
				concluidas++
			case TipoErro:
				erros++
			}
			segundos += item.trabalhando
		}
		dias[i].Concluidas, dias[i].Erros, dias[i].TempoS = concluidas, erros, segundos
		partes := []string{}
		if concluidas > 0 {
			partes = append(partes, plural(concluidas, "concluída", "concluídas"))
		}
		if segundos >= 60 {
			partes = append(partes, "agentes "+Duracao(segundos))
		}
		dias[i].Resumo = strings.Join(partes, " · ")
	}
	return dias
}
