const PREC = {
  cast: 1,
  compare: 2,
  add: 3,
  multiply: 4,
  unary: 5,
  call: 6,
};

module.exports = grammar({
  name: 'garden',

  extras: $ => [
    /[\s\uFEFF\u2060\u200B]+/,
    $.comment,
  ],

  word: $ => $.identifier,

  rules: {
    source_file: $ => repeat($._item),

    _item: $ => choice(
      $.import_statement,
      $.let_statement,
      $.function_declaration,
      $.extern_declaration,
      $.expression,
    ),

    comment: _ => token(seq('#', /[^!\n].*/)),

    doc_comment: _ => token(seq('#!', /.*/)),

    doc_comments: $ => repeat1($.doc_comment),

    import_statement: $ => seq(
      'import',
      choice(
        $.string,
        seq('(', repeat1($.string), ')'),
      ),
    ),

    let_statement: $ => seq(
      optional($.doc_comments),
      'let',
      $.identifier,
      '=',
      $.expression,
    ),

    function_declaration: $ => seq(
      optional($.doc_comments),
      'fn',
      $.identifier,
      '(',
      repeat($.parameter),
      ')',
      optional($.type),
      $.block,
    ),

    extern_declaration: $ => seq(
      optional($.doc_comments),
      'extern',
      $.string,
      '{',
      repeat($.extern_function_declaration),
      '}',
    ),

    extern_function_declaration: $ => seq(
      optional($.doc_comments),
      'fn',
      $.identifier,
      '(',
      repeat($.parameter),
      ')',
      optional($.type),
    ),

    parameter: $ => seq(
      $.identifier,
      ':',
      $.type,
    ),

    block: $ => seq(
      '{',
      repeat($._item),
      '}',
    ),

    match_expression: $ => seq(
      'match',
      '{',
      repeat($.match_arm),
      '}',
    ),

    match_arm: $ => choice(
      seq($.expression, $.block),
      $.block,
    ),

    expression: $ => choice(
      $.match_expression,
      $.cast_expression,
      $.binary_expression,
      $.unary_expression,
      $.call_expression,
      $.field_expression,
      $.primary_expression,
    ),

    cast_expression: $ => prec.left(PREC.cast, seq(
      $.expression,
      'as',
      $.type,
    )),

    binary_expression: $ => choice(
      prec.left(PREC.compare, seq($.expression, choice('==', '!=', '<', '>'), $.expression)),
      prec.left(PREC.add, seq($.expression, choice('+', '-'), $.expression)),
      prec.left(PREC.multiply, seq($.expression, choice('*', '/', '%'), $.expression)),
    ),

    unary_expression: $ => prec.right(PREC.unary, seq(
      choice('+', '-'),
      $.expression,
    )),

    call_expression: $ => prec.left(PREC.call, seq(
      $.expression,
      '(',
      repeat($.expression),
      ')',
    )),

    field_expression: $ => prec.left(PREC.call, seq(
      $.expression,
      '.',
      $.identifier,
    )),

    primary_expression: $ => choice(
      $.identifier,
      $.string,
      $.number,
      $.boolean,
      seq('(', $.expression, ')'),
    ),

    boolean: $ => choice('true', 'false'),

    number: $ => token(choice(
      /\d+\.\d+/,
      /\d+/,
    )),

    string: $ => token(seq(
      '"',
      repeat(choice(
        /[^"\\\n]/,
        /\\./,
      )),
      '"',
    )),

    identifier: $ => /[A-Za-z_][A-Za-z0-9_]*/,

    type: $ => choice(
      $.type_atom,
      $.foreign_type,
      $.option_type,
      $.array_type,
    ),

    type_atom: $ => choice('Str', 'Int', 'Double', 'Bool', 'Void'),

    foreign_type: $ => seq(
      'Foreign',
      '<',
      $.type_identifier,
      '>',
    ),

    option_type: $ => seq(
      'Option',
      '<',
      $.type,
      '>',
    ),

    array_type: $ => seq(
      'Array',
      '<',
      $.type,
      '>',
    ),

    type_identifier: $ => $.identifier,
  },
});
