package linha

import (
	"fmt"
	"sort"
	"strings"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/ferramentas"
)

// Apresentação da daily e da sprint: uma tarefa por slide, com uma capa que
// resume o período. Os textos são os mesmos da linha do tempo, sem o nome
// da tarefa (que já está no título do slide).

const (
	// MaxSlides de tarefa; o que passar vira `mais`, com um slide final.
	MaxSlides = 60
	// Tópicos de "o que foi feito" por parte; a tela mostra até 6 e "+N".
	maxFeito = 20
	// Destaques da capa.
	maxDestaques = 3
	// Títulos citados num destaque (o resto vira "e mais N") e o tamanho de
	// cada um: na capa, a frase não pode perder o verbo por causa de um título.
	maxTitulosDestaque = 3
	maxTituloDestaque  = 40
)

// Grupos dos slides, na ordem em que aparecem.
const (
	GrupoConcluidas  = "concluidas"
	GrupoRevisao     = "revisao"
	GrupoAguardando  = "aguardando"
	GrupoTrabalhando = "trabalhando"
	GrupoErros       = "erros"
	GrupoOutras      = "outras"
)

var ordemGrupos = map[string]int{GrupoConcluidas: 0, GrupoRevisao: 1, GrupoAguardando: 2, GrupoTrabalhando: 3, GrupoErros: 4, GrupoOutras: 5}

// NumerosCapa são os números do período, os mesmos na Daily, na Sprint e na
// capa. Concluidas, Revisao, Aguardando e Trabalhando contam as tarefas de
// cada grupo de slides (os mesmos nomes dos grupos); Erros conta as sessões
// que pararam com erro (como o "N erros" do dia na linha do tempo); Novas
// conta todas as tarefas criadas no período.
type NumerosCapa struct {
	Concluidas  int `json:"concluidas"`
	Revisao     int `json:"revisao"`
	Aguardando  int `json:"aguardando"`
	Trabalhando int `json:"trabalhando"`
	Erros       int `json:"erros"`
	Novas       int `json:"novas"`
	// O tempo dos agentes só vai com a opção do perfil ligada.
	TempoS        int64             `json:"tempo_s,omitempty"`
	PorFerramenta []TempoFerramenta `json:"por_ferramenta,omitempty"`
}

// ParteCapa é uma coluna da capa: um dia na daily (um projeto, quando a
// daily tem mais de um), um projeto na sprint.
type ParteCapa struct {
	Titulo  string  `json:"titulo"`
	Tarefas []int64 `json:"tarefas"`
}

type Capa struct {
	Numeros   NumerosCapa `json:"numeros"`
	Destaques []string    `json:"destaques"`
	Partes    []ParteCapa `json:"partes"`
	// Novas são as tarefas só criadas no período, sem trabalho: não viram slide.
	Novas []string `json:"novas"`
}

// Feito é o que aconteceu com a tarefa numa parte do período.
type Feito struct {
	Parte string   `json:"parte"`
	Itens []string `json:"itens"`
	Mais  int      `json:"mais"`
}

type NumerosSlide struct {
	TempoS   int64 `json:"tempo_s,omitempty"`
	Sessoes  int   `json:"sessoes"`
	Erros    int   `json:"erros"`
	Capturas int   `json:"capturas"`
}

// AnexoSlide é uma imagem ou um vídeo da tarefa no período.
type AnexoSlide struct {
	ID      int64  `json:"id"`
	Tipo    string `json:"tipo"`
	Nome    string `json:"nome,omitempty"`
	Bytes   int    `json:"bytes"`
	Largura int    `json:"largura"`
	Altura  int    `json:"altura"`
}

// NotaAnterior é a última nota de sprint da tarefa, de outro período: a tela
// mostra apagada, para a nota não sumir ao trocar "14 dias" por "este mês".
type NotaAnterior struct {
	Texto   string `json:"texto"`
	Periodo string `json:"periodo"`
}

type Slide struct {
	TarefaID  int64  `json:"tarefa_id"`
	Titulo    string `json:"titulo"`
	ProjetoID int64  `json:"projeto_id"`
	Projeto   string `json:"projeto"`
	Coluna    string `json:"coluna"`
	Status    string `json:"status"`
	Grupo     string `json:"grupo"`
	Removida  bool   `json:"removida"`
	// Secao é o nome do projeto da tarefa ("estudos · loja-web" com mais de
	// um workspace no recorte): com mais de uma seção, a tela põe um título
	// (e um slide divisor) quando muda. SecaoID é o projeto e Workspace, o
	// workspace dele quando o recorte tem mais de um.
	Secao     string `json:"secao,omitempty"`
	SecaoID   int64  `json:"secao_id,omitempty"`
	Workspace string `json:"workspace,omitempty"`
	// Partes em que a tarefa aparece ("Ontem", "Hoje"), para as marcas do cabeçalho.
	Partes  []string     `json:"partes"`
	Feito   []Feito      `json:"feito"`
	Numeros NumerosSlide `json:"numeros"`
	Anexos  []AnexoSlide `json:"anexos"`
	Nota    string       `json:"nota"`
	// NotaVersao é o atualizada_em da nota: a tela manda de volta ao salvar,
	// para não apagar o que o agente complementou enquanto ela editava.
	NotaVersao   string        `json:"nota_versao"`
	NotaAnterior *NotaAnterior `json:"nota_anterior,omitempty"`
	// Lousa da tarefa, quando ela tem itens: a tela busca os itens sob
	// demanda e mostra só leitura, na aba "Lousa" da coluna da direita.
	Lousa *dados.ResumoLousa `json:"lousa,omitempty"`

	ultimo  time.Time
	ultErro bool
	brutos  []bruto
}

// bruto é um tópico antes de juntar: uma frase ou uma sessão de um agente.
type bruto struct {
	parte string
	texto string
	// Sessão que terminou bem: as do mesmo agente na mesma parte viram uma linha.
	agente      string
	trabalhando int64
	aguardando  int64
	// Dia ("25/09") na sprint, para a frase dizer quando foi.
	dia string
}

// Deck é a apresentação inteira.
type Deck struct {
	Tipo    string `json:"tipo"`
	Titulo  string `json:"titulo"`
	Periodo string `json:"periodo"`
	// De e Ate (AAAA-MM-DD) limitam os anexos; ChaveNota é o período das notas.
	De        string  `json:"de"`
	Ate       string  `json:"ate"`
	ChaveNota string  `json:"chave_nota"`
	Capa      Capa    `json:"capa"`
	Slides    []Slide `json:"slides"`
	Mais      int     `json:"mais"`
	Vazio     bool    `json:"vazio"`
	// TempoAgentes: os números e os textos podem ter o tempo dos agentes.
	TempoAgentes bool `json:"tempo_agentes"`
	// Fora: as tarefas tiradas da daily de hoje (só na daily), para trazer de volta.
	Fora []TarefaFora `json:"fora,omitempty"`
}

// TarefaFora é uma tarefa tirada da daily de hoje.
type TarefaFora struct {
	TarefaID int64  `json:"tarefa_id"`
	Titulo   string `json:"titulo"`
	Projeto  string `json:"projeto"`
}

// trabalho diz se o item conta como trabalho na tarefa (uma tarefa só criada
// ou um terminal comum parado não viram slide).
func trabalho(tipo string) bool {
	switch tipo {
	case TipoConcluiu, TipoMoveu, TipoSessao, TipoSessaoAberta, TipoSessaoAguardando, TipoErro, TipoInterrompido, TipoCaptura, TipoNota, TipoLousa,
		TipoBancoAgente:
		return true
	}
	return false
}

// rotuloCurto: "Ontem", "Hoje" ou o dia da semana ("Sexta").
func rotuloCurto(dia, hoje, ontem string, fuso *time.Location) string {
	switch dia {
	case hoje:
		return "Hoje"
	case ontem:
		return "Ontem"
	}
	d, _ := time.ParseInLocation("2006-01-02", dia, fuso)
	nome := diasSemana[d.Weekday()]
	return strings.ToUpper(nome[:1]) + nome[1:]
}

// rotuloLongo: "Ontem (quinta)" ou "Sexta (25/09)".
func rotuloLongo(dia, ontem string, fuso *time.Location) string {
	d, _ := time.ParseInLocation("2006-01-02", dia, fuso)
	if dia == ontem {
		return "Ontem (" + diasSemana[d.Weekday()] + ")"
	}
	nome := diasSemana[d.Weekday()]
	return fmt.Sprintf("%s%s (%s)", strings.ToUpper(nome[:1]), nome[1:], d.Format("02/01"))
}

func diaPorExtenso(t time.Time) string {
	return fmt.Sprintf("%s, %d de %s", diasSemana[t.Weekday()], t.Day(), meses[t.Month()-1])
}

// Apresentacao monta o deck da daily (tipo "daily": o último dia com
// atividade antes de hoje, até JanelaDaily dias atrás, e hoje) ou da sprint
// (tipo "sprint": de `de` a `ate`, inclusive). `eventos` é o mesmo que a
// Daily e a Sprint recebem. Anexos e notas entram depois, por Completar.
func Apresentacao(eventos []dados.Evento, de, ate time.Time, c Contexto, tipo string) Deck {
	m := novoMontador(c)
	itens := m.montar(eventos)
	fuso := m.c.Fuso
	agora := c.Agora.In(fuso)
	hoje := diaDe(agora, fuso)
	ontem := diaDe(agora.AddDate(0, 0, -1), fuso)
	escopo := ""
	if e := c.escopo(); e != "" {
		escopo = " · " + e
	}

	d := Deck{Tipo: tipo, Slides: []Slide{}, Capa: Capa{Destaques: []string{}, Partes: []ParteCapa{}, Novas: []string{}}, TempoAgentes: c.MostrarTempo}
	var inicio, fim string
	// Na daily, as partes são os dias; na sprint, uma parte só.
	partes := map[string]string{}
	if tipo == "daily" {
		limite := diaDe(agora.AddDate(0, 0, -JanelaDaily), fuso)
		ultimo := ""
		for _, item := range itens {
			dia := diaDe(item.quando, fuso)
			if dia < hoje && dia >= limite && item.Tipo != TipoProjeto && !deBanco(item.Tipo) && dia > ultimo {
				ultimo = dia
			}
		}
		inicio, fim = hoje, hoje
		if ultimo != "" {
			inicio = ultimo
			partes[ultimo] = rotuloCurto(ultimo, hoje, ontem, fuso)
		}
		partes[hoje] = "Hoje"
		d.Titulo = "Daily · " + diaPorExtenso(agora)
		if ultimo != "" {
			u, _ := time.ParseInLocation("2006-01-02", ultimo, fuso)
			d.Periodo = "desde " + diaPorExtenso(u) + escopo
		} else {
			d.Periodo = "só hoje" + escopo
		}
		d.ChaveNota = hoje
		// O dia anterior já foi escolhido com tudo: tirar uma tarefa não puxa
		// a daily para um dia mais antigo.
		itens = c.semForaDaDaily(itens)
		for id := range c.ForaDaDaily {
			if t, ok := c.Tarefas[id]; ok {
				d.Fora = append(d.Fora, TarefaFora{TarefaID: id, Titulo: t.Titulo, Projeto: m.nomeProjeto(t.ProjetoID)})
			}
		}
		sort.Slice(d.Fora, func(i, j int) bool { return d.Fora[i].TarefaID < d.Fora[j].TarefaID })
	} else {
		inicio, fim = de.Format("2006-01-02"), ate.Format("2006-01-02")
		// O ano só aparece quando o período cruza a virada do ano; o
		// subtítulo diz a duração e o escopo (as datas já estão no título).
		formato := "02/01"
		if de.Year() != ate.Year() {
			formato = "02/01/2006"
		}
		d.Titulo = fmt.Sprintf("Sprint · %s a %s", de.Format(formato), ate.Format(formato))
		dias := int(ate.Sub(de).Hours()/24+0.5) + 1
		d.Periodo = plural(dias, "dia", "dias") + escopo
		d.ChaveNota = inicio + ".." + fim
	}
	d.De, d.Ate = inicio, fim

	// Coluna de cada tarefa no fim do período (a sprint pode ser do passado).
	coluna := map[int64]string{}
	for _, mc := range m.colunas {
		if diaDe(mc.quando, fuso) > fim {
			break
		}
		coluna[mc.tarefa] = mc.coluna
	}

	slides := map[int64]*Slide{}
	criadas := map[int64]bool{}
	var ordemCriadas []int64
	porFerramenta := map[string]int64{}
	novoSlide := func(id, projeto int64, nomeProjeto string) *Slide {
		if s, ok := slides[id]; ok {
			return s
		}
		s := &Slide{TarefaID: id, Titulo: m.titulo(id), ProjetoID: projeto, Projeto: cmpOr(nomeProjeto, m.nomeProjeto(projeto)), Partes: []string{}, Feito: []Feito{}, Anexos: []AnexoSlide{}}
		slides[id] = s
		return s
	}
	for _, item := range itens {
		dia := diaDe(item.quando, fuso)
		if dia < inicio || dia > fim || item.TarefaID == 0 {
			continue
		}
		parte := "No período"
		if tipo == "daily" {
			var ok bool
			if parte, ok = partes[dia]; !ok {
				continue
			}
		}
		if item.Tipo == TipoCriou && !criadas[item.TarefaID] {
			criadas[item.TarefaID] = true
			ordemCriadas = append(ordemCriadas, item.TarefaID)
		}
		if !trabalho(item.Tipo) {
			continue
		}
		s := novoSlide(item.TarefaID, item.ProjetoID, item.Projeto)
		if item.quando.After(s.ultimo) {
			s.ultimo = item.quando
		}
		if len(s.Partes) == 0 || s.Partes[len(s.Partes)-1] != parte {
			s.Partes = append(s.Partes, parte)
		}
		switch item.Tipo {
		case TipoSessao, TipoSessaoAberta, TipoSessaoAguardando, TipoInterrompido:
			s.Numeros.Sessoes++
			s.ultErro = false
		case TipoErro:
			s.Numeros.Sessoes++
			s.Numeros.Erros++
			s.ultErro = true
		case TipoCaptura:
			s.Numeros.Capturas += len(item.Anexos) - len(item.Videos)
		}
		if item.trabalhando > 0 {
			s.Numeros.TempoS += item.trabalhando
			d.Capa.Numeros.TempoS += item.trabalhando
			porFerramenta[ferramentas.Nome(item.ferramenta)] += item.trabalhando
		}
		// A nota já aparece no slide; anexos e capturas aparecem como mídia
		// e no número de capturas: nada disso vira tópico.
		if item.Tipo == TipoNota || item.Tipo == TipoCaptura || item.Curto == "" {
			continue
		}
		data := ""
		if tipo != "daily" {
			data = item.quando.In(fuso).Format("02/01")
		}
		if item.Tipo == TipoSessao && item.agente != "" {
			s.brutos = append(s.brutos, bruto{parte: parte, agente: item.agente, trabalhando: item.trabalhando, aguardando: item.aguardando, dia: data})
			continue
		}
		texto := item.Curto
		if data != "" {
			texto = data + " · " + texto
		}
		s.brutos = append(s.brutos, bruto{parte: parte, texto: texto})
	}

	// Na daily, o que está aberto agora também entra em Hoje (como no texto da daily).
	if tipo == "daily" {
		abertas := make([]TarefaAtual, 0, len(c.Tarefas))
		for _, t := range c.Tarefas {
			abertas = append(abertas, t)
		}
		sort.Slice(abertas, func(i, j int) bool { return abertas[i].ID < abertas[j].ID })
		for _, t := range abertas {
			if (t.Coluna != "aguardando" && t.Coluna != "trabalhando") || c.ForaDaDaily[t.ID] {
				continue
			}
			if t.Coluna == "trabalhando" && soTerminaisParados(c.Ativos, t.ID) {
				continue
			}
			s := novoSlide(t.ID, t.ProjetoID, "")
			if len(s.Partes) == 0 || s.Partes[len(s.Partes)-1] != "Hoje" {
				s.Partes = append(s.Partes, "Hoje")
				texto := "Segue em andamento."
				if t.Coluna == "aguardando" {
					texto = "Está esperando sua resposta."
				}
				s.brutos = append(s.brutos, bruto{parte: "Hoje", texto: texto})
			}
		}
	}

	ordenados := make([]*Slide, 0, len(slides))
	for _, s := range slides {
		s.Feito = s.juntar(c.MostrarTempo)
		atual, existe := c.Tarefas[s.TarefaID]
		s.Removida = !existe
		s.Coluna = coluna[s.TarefaID]
		if tipo == "daily" && existe {
			s.Coluna = atual.Coluna
		}
		if existe {
			s.Titulo = atual.Titulo
		}
		s.Status = cmpOr(nomesColuna[s.Coluna], s.Coluna)
		switch {
		case s.Removida:
			s.Grupo, s.Status = GrupoOutras, "Removida"
		case s.Coluna == "concluido":
			s.Grupo = GrupoConcluidas
		case s.ultErro:
			s.Grupo = GrupoErros
		case s.Coluna == "revisao" || s.Coluna == "aguardando" || s.Coluna == "trabalhando":
			s.Grupo = s.Coluna
		default:
			s.Grupo = GrupoOutras
		}
		s.Secao, s.SecaoID, s.Workspace = c.rotuloSecao(s.ProjetoID, s.Projeto), s.ProjetoID, c.workspaceDaSecao(s.ProjetoID)
		ordenados = append(ordenados, s)
	}
	// Por projeto (na ordem da barra lateral), depois o grupo e o mais recente.
	sort.Slice(ordenados, func(i, j int) bool {
		a, b := ordenados[i], ordenados[j]
		if a.SecaoID != b.SecaoID || a.Secao != b.Secao {
			return c.antesNaOrdem(a.SecaoID, b.SecaoID, a.Secao, b.Secao)
		}
		if ordemGrupos[a.Grupo] != ordemGrupos[b.Grupo] {
			return ordemGrupos[a.Grupo] < ordemGrupos[b.Grupo]
		}
		if !a.ultimo.Equal(b.ultimo) {
			return a.ultimo.After(b.ultimo)
		}
		return a.TarefaID < b.TarefaID
	})

	// Capa: números, partes, novas e destaques, contando todos os slides
	// (mesmo os que passam do limite).
	var concluidas, esperando, comErro []string
	for _, s := range ordenados {
		d.Capa.Numeros.Erros += s.Numeros.Erros
		switch s.Grupo {
		case GrupoConcluidas:
			d.Capa.Numeros.Concluidas++
			concluidas = append(concluidas, s.Titulo)
		case GrupoRevisao:
			d.Capa.Numeros.Revisao++
		case GrupoTrabalhando:
			d.Capa.Numeros.Trabalhando++
		case GrupoAguardando:
			d.Capa.Numeros.Aguardando++
			esperando = append(esperando, s.Titulo)
		case GrupoErros:
			comErro = append(comErro, s.Titulo)
		}
	}
	d.Capa.Numeros.Novas = len(ordemCriadas)
	for _, id := range ordemCriadas {
		if _, virouSlide := slides[id]; !virouSlide {
			d.Capa.Novas = append(d.Capa.Novas, m.titulo(id))
		}
	}
	if len(ordenados) > MaxSlides {
		d.Mais = len(ordenados) - MaxSlides
		ordenados = ordenados[:MaxSlides]
	}
	// Na daily de um projeto, uma coluna por dia; com mais de um projeto
	// (como na sprint), uma por projeto.
	variasSecoes := false
	for _, s := range ordenados {
		variasSecoes = variasSecoes || s.SecaoID != ordenados[0].SecaoID || s.Secao != ordenados[0].Secao
	}
	if tipo == "daily" && !variasSecoes {
		dias := make([]string, 0, len(partes))
		for dia := range partes {
			dias = append(dias, dia)
		}
		sort.Strings(dias)
		for _, dia := range dias {
			titulo := "Hoje"
			if dia != hoje {
				titulo = rotuloLongo(dia, ontem, fuso)
			}
			p := ParteCapa{Titulo: titulo, Tarefas: []int64{}}
			for _, s := range ordenados {
				for _, parte := range s.Partes {
					if parte == partes[dia] {
						p.Tarefas = append(p.Tarefas, s.TarefaID)
					}
				}
			}
			d.Capa.Partes = append(d.Capa.Partes, p)
		}
	} else {
		var ultimaSecao int64 = -1
		for _, s := range ordenados {
			if n := len(d.Capa.Partes); n == 0 || d.Capa.Partes[n-1].Titulo != s.Secao || ultimaSecao != s.SecaoID {
				ultimaSecao = s.SecaoID
				d.Capa.Partes = append(d.Capa.Partes, ParteCapa{Titulo: s.Secao, Tarefas: []int64{}})
			}
			p := &d.Capa.Partes[len(d.Capa.Partes)-1]
			p.Tarefas = append(p.Tarefas, s.TarefaID)
		}
	}
	// O verbo vem antes dos títulos: se a tela cortar, corta um título, não o sentido.
	if len(concluidas) > 0 {
		d.Capa.Destaques = append(d.Capa.Destaques, "Concluí "+citar(concluidas))
	}
	if len(esperando) > 0 {
		d.Capa.Destaques = append(d.Capa.Destaques, "Esperando minha resposta: "+citar(esperando))
	}
	if len(comErro) > 0 {
		d.Capa.Destaques = append(d.Capa.Destaques, "Tive erro em "+citar(comErro))
	}
	if d.Capa.Numeros.TempoS >= 60 {
		d.Capa.Destaques = append(d.Capa.Destaques, "Agentes trabalharam "+Duracao(d.Capa.Numeros.TempoS))
	}
	if len(d.Capa.Destaques) > maxDestaques {
		d.Capa.Destaques = d.Capa.Destaques[:maxDestaques]
	}
	for nome, segundos := range porFerramenta {
		d.Capa.Numeros.PorFerramenta = append(d.Capa.Numeros.PorFerramenta, TempoFerramenta{Nome: nome, Segundo: segundos})
	}
	sort.Slice(d.Capa.Numeros.PorFerramenta, func(i, j int) bool {
		a, b := d.Capa.Numeros.PorFerramenta[i], d.Capa.Numeros.PorFerramenta[j]
		if a.Segundo != b.Segundo {
			return a.Segundo > b.Segundo
		}
		return a.Nome < b.Nome
	})
	for _, s := range ordenados {
		d.Slides = append(d.Slides, *s)
	}
	d.Vazio = len(d.Slides) == 0 && len(d.Capa.Novas) == 0
	return d
}

// citar: até 3 títulos entre aspas, cada um cortado em cerca de 40
// caracteres, e "e mais N" no fim.
func citar(titulos []string) string {
	nomes := []string{}
	for i, t := range titulos {
		if i == maxTitulosDestaque {
			nomes = append(nomes, fmt.Sprintf("mais %d", len(titulos)-maxTitulosDestaque))
			break
		}
		nomes = append(nomes, "“"+encurtar(t, maxTituloDestaque)+"”")
	}
	return lista(nomes)
}

// encurtar corta o texto em até `maximo` caracteres, de preferência entre
// palavras, sem vírgula ou espaço antes do "…".
func encurtar(texto string, maximo int) string {
	runas := []rune(texto)
	if len(runas) <= maximo {
		return texto
	}
	corte := string(runas[:maximo])
	// Corta entre palavras, a não ser que a última palavra fique longe demais.
	if i := strings.LastIndex(corte, " "); runas[maximo] != ' ' && i > len(corte)*3/5 {
		corte = corte[:i]
	}
	return strings.TrimRight(corte, " ,;:·-–") + "…"
}

// juntar monta "o que foi feito" de cada parte. A mesma frase repetida (seguida
// ou não) vira uma linha com "(N vezes)", e as sessões de um agente viram uma
// linha só: "Claude Code (dev) trabalhou 1h10 em 7 sessões" (sem o tempo dos
// agentes, "Claude Code (dev): 7 sessões"). Parte sem tópico não aparece.
func (s *Slide) juntar(mostrarTempo bool) []Feito {
	type linha struct {
		texto, agente           string
		vezes                   int
		trabalhando, aguardando int64
		primeiro, ultimo        string
	}
	var partes []string
	porParte := map[string][]*linha{}
	for _, b := range s.brutos {
		lista, existe := porParte[b.parte]
		if !existe {
			partes = append(partes, b.parte)
		}
		texto := strings.TrimSuffix(b.texto, ".")
		var achou *linha
		for _, l := range lista {
			if l.agente == b.agente && (b.agente != "" || l.texto == texto) {
				achou = l
				break
			}
		}
		if achou == nil {
			achou = &linha{texto: texto, agente: b.agente, primeiro: b.dia}
			porParte[b.parte] = append(lista, achou)
		}
		achou.vezes++
		achou.trabalhando += b.trabalhando
		achou.aguardando += b.aguardando
		achou.ultimo = b.dia
	}
	feito := []Feito{}
	for _, parte := range partes {
		f := Feito{Parte: parte, Itens: []string{}}
		for _, l := range porParte[parte] {
			if len(f.Itens) >= maxFeito {
				f.Mais++
				continue
			}
			if l.agente == "" {
				texto := l.texto
				if l.vezes > 1 {
					texto += fmt.Sprintf(" (%d vezes)", l.vezes)
				}
				f.Itens = append(f.Itens, texto+".")
				continue
			}
			var texto string
			switch {
			case !mostrarTempo && l.vezes > 1:
				texto = fmt.Sprintf("%s: %d sessões", l.agente, l.vezes)
			case !mostrarTempo:
				texto = l.agente + " terminou uma sessão"
			default:
				texto = l.agente + " trabalhou " + Duracao(l.trabalhando)
				if l.vezes > 1 {
					texto += fmt.Sprintf(" em %d sessões", l.vezes)
				}
				if l.aguardando >= 60 {
					texto += " e esperou você " + Duracao(l.aguardando)
				}
			}
			switch {
			case l.primeiro == "":
			case l.primeiro == l.ultimo:
				texto = l.primeiro + " · " + texto
			default:
				texto = l.primeiro + " a " + l.ultimo + " · " + texto
			}
			f.Itens = append(f.Itens, texto+".")
		}
		feito = append(feito, f)
	}
	return feito
}

// Tarefas dos slides, na ordem.
func (d *Deck) Tarefas() []int64 {
	ids := make([]int64, len(d.Slides))
	for i, s := range d.Slides {
		ids[i] = s.TarefaID
	}
	return ids
}

// Completar põe nos slides os anexos (não removidos, do mais novo ao mais
// antigo) e as notas. `anteriores` são as últimas notas de sprint de outros
// períodos, para quando não há nota neste.
func (d *Deck) Completar(anexos []dados.Anexo, notas, anteriores map[int64]dados.Nota) {
	porTarefa := map[int64][]AnexoSlide{}
	for _, a := range anexos {
		if a.Removido || a.TarefaID == 0 {
			continue
		}
		porTarefa[a.TarefaID] = append([]AnexoSlide{{ID: a.ID, Tipo: a.Tipo, Nome: a.Nome, Bytes: a.Bytes, Largura: a.Largura, Altura: a.Altura}}, porTarefa[a.TarefaID]...)
	}
	for i := range d.Slides {
		s := &d.Slides[i]
		if lista, ok := porTarefa[s.TarefaID]; ok {
			s.Anexos = lista
			s.Numeros.Capturas = 0
			for _, a := range lista {
				if a.Tipo == "imagem" {
					s.Numeros.Capturas++
				}
			}
		}
		s.Nota, s.NotaVersao = notas[s.TarefaID].Texto, notas[s.TarefaID].AtualizadaEm
		if n, ok := anteriores[s.TarefaID]; ok && s.Nota == "" && n.Periodo != d.ChaveNota && n.Texto != "" {
			s.NotaAnterior = &NotaAnterior{Texto: n.Texto, Periodo: periodoLegivel(n.Periodo)}
		}
	}
}

// CompletarLousas põe em cada slide a lousa da tarefa que tem itens.
func (d *Deck) CompletarLousas(lousas map[int64]dados.ResumoLousa) {
	for i := range d.Slides {
		if l, ok := lousas[d.Slides[i].TarefaID]; ok && l.Elementos > 0 {
			d.Slides[i].Lousa = &l
		}
	}
}

// periodoLegivel: "2026-09-21..2026-10-02" vira "21/09 a 02/10".
func periodoLegivel(p string) string {
	de, ate, ok := strings.Cut(p, "..")
	if !ok {
		return p
	}
	curto := func(v string) string {
		t, err := time.Parse("2006-01-02", v)
		if err != nil {
			return v
		}
		return t.Format("02/01")
	}
	return curto(de) + " a " + curto(ate)
}
