var _yt_player = {};
(function(g) {
    g.signatureTimestamp = 19842;
    var kx = function(a) {
        a = a.split("");
        AB.ro(a, 2);
        AB.xh(a, 5);
        AB.mn(a, 3);
        return a.join("");
    };
    var AB = {
        ro: function(a) { a.reverse() },
        xh: function(a, b) { var c = a[0]; a[0] = a[b % a.length]; a[b % a.length] = c; },
        mn: function(a, b) { a.splice(0, b); }
    };
    c && a.set("sig", encodeURIComponent(kx(c)));
    var b = a.get("n");
    if (b) { b = nTrans(b); }
})(this);
