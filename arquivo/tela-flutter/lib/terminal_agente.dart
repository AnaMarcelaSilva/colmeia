import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:web_socket_channel/web_socket_channel.dart';
import 'package:xterm/xterm.dart';

import 'metricas.dart';

const _confirmarACada = 64 * 1024;

const _tema = TerminalTheme(
  cursor: Color(0xFFC792EA),
  selection: Color(0xFF3A3F4B),
  foreground: Color(0xFFD7DAE0),
  background: Color(0xFF0F1115),
  black: Color(0xFF000000),
  red: Color(0xFFCD3131),
  green: Color(0xFF0DBC79),
  yellow: Color(0xFFE5E510),
  blue: Color(0xFF2472C8),
  magenta: Color(0xFFBC3FBC),
  cyan: Color(0xFF11A8CD),
  white: Color(0xFFE5E5E5),
  brightBlack: Color(0xFF666666),
  brightRed: Color(0xFFF14C4C),
  brightGreen: Color(0xFF23D18B),
  brightYellow: Color(0xFFF5F543),
  brightBlue: Color(0xFF3B8EEA),
  brightMagenta: Color(0xFFD670D6),
  brightCyan: Color(0xFF29B8DB),
  brightWhite: Color(0xFFFFFFFF),
  searchHitBackground: Color(0xFFFFFF2B),
  searchHitBackgroundCurrent: Color(0xFF31FF26),
  searchHitForeground: Color(0xFF000000),
);

/// Recebe texto já decodificado e escreve no terminal.
class _Escritor implements Sink<String> {
  _Escritor(this.terminal);
  final Terminal terminal;

  @override
  void add(String data) => terminal.write(data);

  @override
  void close() {}
}

class TerminalAgente extends StatefulWidget {
  const TerminalAgente({super.key, required this.id});
  final int id;

  @override
  State<TerminalAgente> createState() => _TerminalAgenteState();
}

class _TerminalAgenteState extends State<TerminalAgente> {
  final terminal = Terminal(maxLines: 1000);
  late final WebSocketChannel canal;
  // Decodifica UTF-8 em pedaços, sem quebrar caracteres divididos entre mensagens.
  late final ByteConversionSink decodificador;
  var desenhados = 0;

  @override
  void initState() {
    super.initState();
    decodificador = const Utf8Decoder(
      allowMalformed: true,
    ).startChunkedConversion(_Escritor(terminal));
    canal = WebSocketChannel.connect(
      Uri.parse('ws://$nucleo/terminal?id=${widget.id}'),
    );

    canal.stream.listen((mensagem) {
      final dados = mensagem as List<int>;
      metricas.bytes += dados.length;
      decodificador.add(dados);
      // Controle de fluxo: confirma ao núcleo o que já foi processado.
      desenhados += dados.length;
      if (desenhados >= _confirmarACada) {
        canal.sink.add(jsonEncode({'ack': desenhados}));
        desenhados = 0;
      }
    }, onError: (_) {});

    terminal.onOutput = (texto) => canal.sink.add(utf8.encode(texto));
    terminal.onResize =
        (colunas, linhas, _, __) =>
            canal.sink.add(jsonEncode({'cols': colunas, 'rows': linhas}));
  }

  @override
  void dispose() {
    canal.sink.close();
    decodificador.close();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    // Isola o redesenho: um terminal atualizando não repinta os outros.
    return RepaintBoundary(
      child: TerminalView(
        terminal,
        theme: _tema,
        padding: const EdgeInsets.all(4),
        textStyle: const TerminalStyle(
          fontSize: 12,
          fontFamily: 'JetBrains Mono',
        ),
      ),
    );
  }
}
