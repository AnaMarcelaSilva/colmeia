package api

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"net/http"
	"sync"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/comandovoz"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
)

// A lista numerada da abelha: chamada (pelo nome ou no clique), ela fala o
// que importa agora com números, e o comando seguinte usa o número ("abrir a
// um"). A lista fica guardada aqui, presa ao perfil, por pouco tempo e com
// uma ficha: os números valem para o que foi falado, mesmo que o quadro
// mude no meio. Uma lista nova troca a anterior.

// Sem uso por este tempo, a lista deixa de valer.
const validadeLista = 2 * time.Minute

// Quantas vezes a abelha pergunta de novo quando não entende.
const tentativasLista = 1

type sessaoLista struct {
	ficha      string
	itens      []comandovoz.ItemLista
	pagina     int
	tentativas int
	expira     time.Time
}

type listas struct {
	mu     sync.Mutex
	perfil map[int64]sessaoLista
}

func (l *listas) abrir(perfil int64, itens []comandovoz.ItemLista) sessaoLista {
	b := make([]byte, 16)
	rand.Read(b)
	s := sessaoLista{ficha: hex.EncodeToString(b), itens: itens, expira: time.Now().Add(validadeLista)}
	l.guardar(perfil, s)
	return s
}

func (l *listas) guardar(perfil int64, s sessaoLista) {
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.perfil == nil {
		l.perfil = map[int64]sessaoLista{}
	}
	s.expira = time.Now().Add(validadeLista)
	l.perfil[perfil] = s
}

// pegar devolve a lista do perfil se a ficha for a dela e ainda valer.
func (l *listas) pegar(perfil int64, ficha string) (sessaoLista, bool) {
	l.mu.Lock()
	defer l.mu.Unlock()
	s, ok := l.perfil[perfil]
	if !ok || s.ficha != ficha || time.Now().After(s.expira) {
		return sessaoLista{}, false
	}
	return s, true
}

func (l *listas) fechar(perfil int64) {
	l.mu.Lock()
	defer l.mu.Unlock()
	delete(l.perfil, perfil)
}

// itemVoz é um item da página, como o balão mostra.
type itemVoz struct {
	Numero   int    `json:"numero"`
	Titulo   string `json:"titulo"`
	Detalhe  string `json:"detalhe"`
	Grupo    string `json:"grupo"`
	TarefaID int64  `json:"tarefa_id"`
}

type listaVoz struct {
	Itens   []itemVoz `json:"itens"`
	Linhas  []string  `json:"linhas"`
	Pagina  int       `json:"pagina"`
	Paginas int       `json:"paginas"`
	Dica    string    `json:"dica"`
}

// Com uma pergunta no ar, o Whisper ouve com as frases da gramática (sim e
// não estão nela): sozinhos, "sim" e "não" curtos saíam piores.
const vocabularioConfirmar = comandovoz.VocabularioDaLista

func (s *Servidor) rotasListaVoz(mux *http.ServeMux) {
	mux.HandleFunc("POST /v1/perfis/{id}/voz/lista", s.listaFalada)
}

func (s *Servidor) listaFalada(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	perfil, err := s.Banco.Perfil(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	if !perfil.Comandos {
		responderErro(w, dados.ErrInvalido{Motivo: "Os comandos falados estão desligados. Ligue em Ajustes › Voz."})
		return
	}
	resposta, err := s.abrirLista(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, resposta)
}

// abrirLista monta a lista com o quadro de agora e fala a primeira página.
func (s *Servidor) abrirLista(ctx context.Context, perfil int64) (respostaVoz, error) {
	q, err := s.quadroFalado(ctx, perfil)
	if err != nil {
		return respostaVoz{}, err
	}
	s.confirmacoes.tirar(perfil, "")
	return s.paginaFalada(s.listas.abrir(perfil, comandovoz.MontarLista(q))), nil
}

// paginaFalada é a resposta com a página da lista: a fala, os itens para o
// balão e o microfone aberto em seguida, com o vocabulário da gramática.
func (s *Servidor) paginaFalada(sessao sessaoLista) respostaVoz {
	pagina := comandovoz.Pagina(sessao.itens, sessao.pagina)
	l := &listaVoz{Pagina: sessao.pagina + 1, Paginas: comandovoz.Paginas(sessao.itens), Dica: comandovoz.DicaDaLista, Itens: []itemVoz{}, Linhas: []string{}}
	for n, item := range pagina {
		l.Itens = append(l.Itens, itemVoz{Numero: n + 1, Titulo: item.Titulo, Detalhe: item.Detalhe, Grupo: item.Grupo.String(), TarefaID: item.TarefaID})
		l.Linhas = append(l.Linhas, item.Linha(n+1))
	}
	r := respostaVoz{Texto: comandovoz.FalaDaLista(sessao.itens, sessao.pagina), Sessao: sessao.ficha, Lista: l, Ouvir: true}
	if len(pagina) == 0 {
		// Lista vazia: o microfone abre para os comandos por nome, com o vocabulário do perfil.
		l.Dica = "Diga um pedido, como rodar e o nome do projeto, ou fechar."
	} else {
		r.Prompt = comandovoz.VocabularioDaLista
	}
	return r
}

// comandoDaLista entende a frase dita com a lista aberta. Devolve falso
// quando ela não é da lista (um comando por nome ou a pergunta livre): aí
// a lista fecha e a frase segue o caminho de sempre.
func (s *Servidor) comandoDaLista(ctx context.Context, perfil dados.Perfil, sessao sessaoLista, texto string) (respostaVoz, bool) {
	pagina := comandovoz.Pagina(sessao.itens, sessao.pagina)
	pergunta := s.confirmacoes.tem(perfil.ID)
	e := comandovoz.EscolherDaLista(texto, perfil.MascoteNome, len(pagina), pergunta)
	if e.Certa {
		return s.agirNaLista(ctx, perfil.ID, sessao, pagina, e), true
	}
	p := comandovoz.Entender(texto, perfil.MascoteNome)
	switch p.Intencao {
	case comandovoz.SoNome:
		// Chamou de novo: repete a página.
		return s.paginaFalada(sessao), true
	case comandovoz.Sim, comandovoz.Nao, comandovoz.OQueEspera:
		s.listas.fechar(perfil.ID)
		return respostaVoz{}, false
	case comandovoz.Rodar, comandovoz.Parar, comandovoz.Encerrar, comandovoz.Abrir, comandovoz.ComoEsta:
		// "abrir a hum" mal transcrito não vira a busca de uma tarefa chamada "hum".
		if !comandovoz.PareceNumero(p.Alvo) {
			s.listas.fechar(perfil.ID)
			return respostaVoz{}, false
		}
	case comandovoz.Desconhecida:
		if perfil.PerguntaIA && e.Nota < comandovoz.PareceLista {
			s.listas.fechar(perfil.ID)
			return respostaVoz{}, false
		}
	}
	return s.naoEntendi(perfil.ID, sessao, len(pagina), e.ForaDaLista), true
}

// naoEntendi pergunta de novo uma vez; na segunda, a lista fecha.
func (s *Servidor) naoEntendi(perfil int64, sessao sessaoLista, quantos, fora int) respostaVoz {
	if sessao.tentativas >= tentativasLista {
		s.listas.fechar(perfil)
		return respostaVoz{Texto: "Não entendi de novo. Me chame quando quiser."}
	}
	sessao.tentativas++
	s.listas.guardar(perfil, sessao)
	texto := "Não entendi. Diga, por exemplo: abrir a um."
	switch {
	case quantos == 0:
		texto = "Não entendi. Diga um pedido, como rodar e o nome do projeto, ou fechar."
	case fora > 0 && quantos == 1:
		texto = "A lista só tem o número um. Diga, por exemplo: abrir a um."
	case fora > 0:
		texto = "A lista vai até o " + numeroFalado(quantos) + ". Diga, por exemplo: abrir a um."
	}
	return s.continuarLista(sessao, respostaVoz{Texto: texto})
}

// continuarLista deixa a lista aberta e o microfone abrindo de novo.
func (s *Servidor) continuarLista(sessao sessaoLista, r respostaVoz) respostaVoz {
	r.Sessao, r.Ouvir = sessao.ficha, true
	if len(comandovoz.Pagina(sessao.itens, sessao.pagina)) > 0 {
		r.Prompt = comandovoz.VocabularioDaLista
	}
	return r
}

func numeroFalado(n int) string {
	return [...]string{"zero", "um", "dois", "três", "quatro", "cinco", "seis", "sete", "oito", "nove"}[min(max(n, 0), 9)]
}

func (s *Servidor) agirNaLista(ctx context.Context, perfil int64, sessao sessaoLista, pagina []comandovoz.ItemLista, e comandovoz.Escolha) respostaVoz {
	switch e.Verbo {
	case comandovoz.VProxima, comandovoz.VAnterior:
		passo := 1
		if e.Verbo == comandovoz.VAnterior {
			passo = -1
		}
		nova := sessao.pagina + passo
		if nova < 0 || nova >= comandovoz.Paginas(sessao.itens) {
			texto := "Essa é a última página."
			if passo < 0 {
				texto = "Essa já é a primeira página."
			}
			return s.continuarLista(sessao, respostaVoz{Texto: texto})
		}
		sessao.pagina, sessao.tentativas = nova, 0
		s.listas.guardar(perfil, sessao)
		return s.paginaFalada(sessao)
	case comandovoz.VRepetir:
		return s.paginaFalada(sessao)
	case comandovoz.VFechar:
		s.listas.fechar(perfil)
		s.confirmacoes.tirar(perfil, "")
		return respostaVoz{Texto: "Tudo bem."}
	case comandovoz.VSim:
		s.listas.fechar(perfil)
		a, valida := s.confirmacoes.tirar(perfil, "")
		if !valida {
			return respostaVoz{Texto: "Essa pergunta já não vale. Me chame de novo."}
		}
		return s.executarFalado(ctx, perfil, a)
	case comandovoz.VNao:
		s.listas.fechar(perfil)
		s.confirmacoes.tirar(perfil, "")
		return respostaVoz{Texto: "Tudo bem, não fiz nada."}
	}
	if e.Numero < 1 || e.Numero > len(pagina) {
		return s.naoEntendi(perfil, sessao, len(pagina), e.Numero)
	}
	q, err := s.quadroFalado(ctx, perfil)
	if err != nil {
		return respostaVoz{Texto: "Não consegui: " + mensagemDeErro(err)}
	}
	resposta := comandovoz.AcaoDaLista(e.Verbo, pagina[e.Numero-1], q)
	switch {
	case resposta.Acao != nil && resposta.Confirmar:
		// A lista continua aberta para o "sim" falado.
		sessao.tentativas = 0
		s.listas.guardar(perfil, sessao)
		r := respostaVoz{Texto: resposta.Texto, Confirmar: s.confirmacoes.guardar(perfil, *resposta.Acao), Sessao: sessao.ficha, Ouvir: true, Prompt: vocabularioConfirmar}
		return r
	case resposta.Acao != nil:
		s.listas.fechar(perfil)
		return s.executarFalado(ctx, perfil, *resposta.Acao)
	}
	s.listas.fechar(perfil)
	return respostaVoz{Texto: resposta.Texto, Ouvir: resposta.Ouvir}
}
