/**
 * Approximation of eslint-plugin-react-hooks (v7) `set-state-in-effect`:
 * flags setState calls executed synchronously in a useEffect callback body.
 * Catches the "cascading render" class (direct calls before any await).
 */
export default {
  meta: {
    name: "local/set-state-in-effect",
    category: "correctness",
  },
  create(context) {
    function isDirectSetStateCall(node, setterNames) {
      return (
        node.type === "CallExpression" &&
        node.callee.type === "Identifier" &&
        setterNames.has(node.callee.name)
      );
    }
    function collectSetterNames(root) {
      // useState destructuring: const [x, setX] = useState(...)
      const names = new Set();
      if (!root || root.type !== "Program") return names;
      for (const stmt of root.body) {
        if (stmt.type !== "ImportDeclaration") continue;
        for (const spec of stmt.specifiers) {
          if (spec.type !== "ImportSpecifier" || spec.imported.name !== "useState") continue;
          // no reliable local walk here; handled via visitor instead
        }
      }
      return names;
    }
    return {
      VariableDeclarator(node) {
        context.setStateNames = context.setStateNames ?? new Set();
        if (
          node.init &&
          node.init.type === "CallExpression" &&
          node.init.callee.type === "Identifier" &&
          node.init.callee.name === "useState" &&
          node.id.type === "ArrayPattern" &&
          node.id.elements[1] &&
          node.id.elements[1].type === "Identifier"
        ) {
          context.setStateNames.add(node.id.elements[1].name);
        }
      },
      "CallExpression[callee.name='useEffect']"(node) {
        const names = context.setStateNames ?? new Set();
        const callback = node.arguments[0];
        if (!callback || callback.type !== "ArrowFunctionExpression") return;
        const body = callback.body;
        const stmts = body.type === "BlockStatement" ? body.body : [{ type: "ExpressionStatement", expression: body }];
        const seenAwait = { value: false };
        for (const stmt of stmts) {
          if (stmt.type !== "ExpressionStatement") continue;
          const expr = stmt.expression;
          if (expr.type === "AwaitExpression") { seenAwait.value = true; continue; }
          if (!seenAwait.value && isDirectSetStateCall(expr, names)) {
            context.report({
              node: expr,
              message: "setState called synchronously inside useEffect — use an event handler or defer past an await.",
            });
          }
        }
      },
    };
  },
};
