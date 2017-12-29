
instructions = set()

names = [
    "rtrue", "rfalse", "print", "print_ret", "nop", "save", "restore", "restart", "ret_popped", "pop", "quit", "new_line", "show_status", "verify", "extended", "piracy",
    "jz", "get_sibling", "get_child", "get_parent", "get_prop_len", "inc", "dec", "print_addr", "call_1s", "remove_obj", "print_obj", "ret", "jump", "print_paddr", "load", "not", "call_1n",
    "none", "je", "jl", "jg", "dec_chk", "inc_chk", "jin", "test", "or", "and", "test_attr", "set_attr", "clear_attr", "store", "insert_obj", "loadw", "loadb", "get_prop", "get_prop_addr", "get_next_prop", "add", "sub", "mul", "div", "mod", "call_2s", "call_2n", "set_colour", "throw",
    "call", "storew", "storeb", "put_prop", "sread", "print_char", "print_num", "random", "push", "pull", "split_window", "set_window", "call_vs2", "erase_window", "erase_line", "set_cursor", "get_cursor", "set_text_style", "buffer_mode", "output_stream", "input_stream", "sound_effect", "read_char", "scan_table", "not_v4", "call_vn", "call_vn2", "tokenise", "encode_text", "copy_table", "print_table", "check_arg_count",
]

implemented = set(['call','add','je','sub','jz','storew','ret','loadw','jump',
    'put_prop','store','test_attr','print','new_line','loadb','and','print_num',
    'inc_chk','print_char','rtrue','insert_obj','push','pull','set_attr','jin',
    'print_obj','get_parent','get_prop','jg','get_child','get_sibling','rfalse',
    'inc','jl','ret_popped','sread','dec_chk','mul','test','storeb','clear_attr',
    'get_prop_addr','get_prop_len','print_paddr','dec','print_ret','div',
    'print_addr', 'not', 'or', 'mod', 'remove_obj', 'random', "get_next_prop",
    "load"])

with open('zork.txt') as f:
    for l in f.readlines():
        if l.strip() == "[End of code]":
            break
        l = l.rstrip()[31:]
        if l:
            i = l.split()[0]
            if i.lower() in names:
                instructions.add(i.lower())

print len(instructions), 'unique instructions in zork.z3'
print 'still unimplemented:', list(instructions - implemented)
