import 'dart:async';

import 'package:flutter/material.dart';
import 'package:http/http.dart' as http;

import 'metricas.dart';
import 'terminal_agente.dart';

const fundo = Color(0xFF0B0D10);
const painel = Color(0xFF14171C);
const borda = Color(0xFF262A33);
const texto = Color(0xFFD7DAE0);
const suave = Color(0xFF8A90A0);
const destaque = Color(0xFFC792EA);
const ok = Color(0xFF7FD18B);

const papeis = [
  'líder',
  'dev',
  'dev',
  'revisor',
  'testador',
  'dev',
  'dev',
  'revisor',
  'testador',
  'dev',
];
const projetos = ['loja-web', 'api-pedidos', 'estudos-rust'];

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  metricas.iniciar();
  runApp(const Prototipo());
}

class Prototipo extends StatelessWidget {
  const Prototipo({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Protótipo · Flutter + xterm.dart',
      debugShowCheckedModeBanner: false,
      theme: ThemeData.dark().copyWith(
        scaffoldBackgroundColor: fundo,
        textTheme: ThemeData.dark().textTheme.apply(
          bodyColor: texto,
          fontSizeFactor: 0.9,
        ),
      ),
      home: const Tela(),
    );
  }
}

class Tela extends StatefulWidget {
  const Tela({super.key});

  @override
  State<Tela> createState() => _TelaState();
}

class _TelaState extends State<Tela> {
  var carga = 'parada';

  void aplicarCarga(String modo) {
    setState(() => carga = modo);
    http.get(Uri.parse('http://$nucleo/carga?modo=$modo')).ignore();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: Row(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const Lateral(),
          Expanded(
            child: Column(
              children: [
                Topo(carga: carga, aoEscolher: aplicarCarga),
                const Padding(
                  padding: EdgeInsets.fromLTRB(16, 12, 16, 4),
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.baseline,
                    textBaseline: TextBaseline.alphabetic,
                    children: [
                      Text(
                        'Nova tela de pedidos',
                        style: TextStyle(
                          fontSize: 15,
                          fontWeight: FontWeight.w700,
                        ),
                      ),
                      SizedBox(width: 12),
                      Text('Agente trabalhando', style: TextStyle(color: ok)),
                      Spacer(),
                      Text(
                        'Flutter + xterm.dart',
                        style: TextStyle(color: suave),
                      ),
                    ],
                  ),
                ),
                Expanded(child: Grade(ativos: carga != 'parada')),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class Lateral extends StatelessWidget {
  const Lateral({super.key});

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 200,
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 16),
      decoration: const BoxDecoration(
        color: painel,
        border: Border(right: BorderSide(color: borda)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Text(
            'Protótipo',
            style: TextStyle(color: destaque, fontWeight: FontWeight.w700),
          ),
          const SizedBox(height: 20),
          const Text(
            'Profissional',
            style: TextStyle(fontWeight: FontWeight.w600),
          ),
          const Padding(
            padding: EdgeInsets.fromLTRB(8, 4, 0, 8),
            child: Text('Empresa X', style: TextStyle(color: suave)),
          ),
          for (final (i, p) in projetos.indexed)
            Container(
              width: double.infinity,
              padding: const EdgeInsets.fromLTRB(16, 6, 8, 6),
              decoration: BoxDecoration(
                color: i == 0 ? const Color(0xFF1F2430) : null,
                borderRadius: BorderRadius.circular(6),
              ),
              child: Text(p, style: TextStyle(color: i == 0 ? texto : suave)),
            ),
          const Padding(
            padding: EdgeInsets.fromLTRB(16, 6, 8, 6),
            child: Text('+ Novo projeto', style: TextStyle(color: destaque)),
          ),
        ],
      ),
    );
  }
}

class Topo extends StatelessWidget {
  const Topo({super.key, required this.carga, required this.aoEscolher});
  final String carga;
  final void Function(String) aoEscolher;

  @override
  Widget build(BuildContext context) {
    const separador = Text('  ›  ', style: TextStyle(color: suave));
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
      decoration: const BoxDecoration(
        border: Border(bottom: BorderSide(color: borda)),
      ),
      child: Row(
        children: [
          const Text('Profissional'),
          separador,
          const Text('Empresa X'),
          separador,
          const Text(
            'loja-web',
            style: TextStyle(fontWeight: FontWeight.w700),
          ),
          const SizedBox(width: 12),
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 2),
            decoration: BoxDecoration(
              color: const Color(0xFF1F2430),
              border: Border.all(color: borda),
              borderRadius: BorderRadius.circular(12),
            ),
            child: const Text('branch: todas ▾'),
          ),
          const Spacer(),
          const Medidor(),
          const SizedBox(width: 24),
          for (final modo in ['parada', 'leve', 'pesada'])
            Padding(
              padding: const EdgeInsets.only(left: 6),
              child: OutlinedButton(
                onPressed: () => aoEscolher(modo),
                style: OutlinedButton.styleFrom(
                  foregroundColor: carga == modo ? destaque : texto,
                  side: BorderSide(color: carga == modo ? destaque : borda),
                  padding: const EdgeInsets.symmetric(
                    horizontal: 10,
                    vertical: 12,
                  ),
                  minimumSize: Size.zero,
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(6),
                  ),
                ),
                child: Text(modo == 'parada' ? 'Parar' : 'Carga $modo'),
              ),
            ),
        ],
      ),
    );
  }
}

/// Só este widget é redesenhado a cada segundo.
class Medidor extends StatefulWidget {
  const Medidor({super.key});

  @override
  State<Medidor> createState() => _MedidorState();
}

class _MedidorState extends State<Medidor> {
  late final Timer relogio;
  var fps = 0, vazao = 0;
  var bytes = metricas.bytes, quadros = metricas.quadros;

  @override
  void initState() {
    super.initState();
    relogio = Timer.periodic(const Duration(seconds: 1), (_) {
      setState(() {
        fps = metricas.quadros - quadros;
        vazao = metricas.bytes - bytes;
      });
      bytes = metricas.bytes;
      quadros = metricas.quadros;
    });
  }

  @override
  void dispose() {
    relogio.cancel();
    super.dispose();
  }

  String formatar(int b) {
    if (b >= 1024 * 1024) return '${(b / 1024 / 1024).toStringAsFixed(1)} MB/s';
    if (b >= 1024) return '${(b / 1024).toStringAsFixed(0)} KB/s';
    return '$b B/s';
  }

  @override
  Widget build(BuildContext context) {
    const forte = TextStyle(color: texto, fontWeight: FontWeight.w700);
    return DefaultTextStyle.merge(
      style: const TextStyle(
        color: suave,
        fontFeatures: [FontFeature.tabularFigures()],
      ),
      child: Row(
        children: [
          Text('$fps', style: forte),
          const Text(' FPS'),
          const SizedBox(width: 16),
          Text(formatar(vazao), style: forte),
          const Text(' recebidos'),
        ],
      ),
    );
  }
}

class Grade extends StatelessWidget {
  const Grade({super.key, required this.ativos});
  final bool ativos;

  @override
  Widget build(BuildContext context) {
    Widget cartao(int id) => Expanded(
      child: Container(
        margin: const EdgeInsets.all(5),
        clipBehavior: Clip.antiAlias,
        decoration: BoxDecoration(
          color: const Color(0xFF0F1115),
          border: Border.all(color: borda),
          borderRadius: BorderRadius.circular(8),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
              decoration: const BoxDecoration(
                color: painel,
                border: Border(bottom: BorderSide(color: borda)),
              ),
              child: Row(
                children: [
                  Container(
                    width: 7,
                    height: 7,
                    decoration: BoxDecoration(
                      color: ativos ? ok : suave,
                      shape: BoxShape.circle,
                    ),
                  ),
                  const SizedBox(width: 6),
                  Text('agente-$id '),
                  Text('· ${papeis[id]}', style: const TextStyle(color: suave)),
                ],
              ),
            ),
            // A chave mantém o terminal vivo quando o cabeçalho muda.
            Expanded(child: TerminalAgente(key: ValueKey(id), id: id)),
          ],
        ),
      ),
    );

    return Padding(
      padding: const EdgeInsets.fromLTRB(11, 7, 11, 11),
      child: Column(
        children: [
          for (final linha in [0, 1])
            Expanded(
              child: Row(
                children: [for (var c = 0; c < 5; c++) cartao(linha * 5 + c)],
              ),
            ),
        ],
      ),
    );
  }
}
