/*
 * Runtime.js
 * Minimal Objective-J Runtime
 *
 * Based on the Cappuccino framework runtime by Francisco Tolmasky
 * Original Copyright 2008-2010, 280 North, Inc.
 * Licensed under the GNU Lesser General Public License v2.1
 */

// Class info flags
var CLS_CLASS = 0x1,
    CLS_META = 0x2,
    CLS_INITIALIZED = 0x4,
    CLS_INITIALIZING = 0x8;

// Global constants
var nil = null,
    Nil = null,
    NULL = null,
    YES = true,
    NO = false;

// UID generator
var _objj_UID = 0;
function objj_generateObjectUID() {
    return _objj_UID++;
}

// Registered classes and protocols
var REGISTERED_CLASSES = Object.create(null);
var REGISTERED_PROTOCOLS = Object.create(null);

// =============================================================================
// Core Types
// =============================================================================

function objj_ivar(aName, aType) {
    this.name = aName;
    this.type = aType;
}

function objj_method(aName, anImplementation, types) {
    this.method_name = aName;
    this.method_imp = anImplementation;
    this.method_types = types;
}

function objj_class(displayName) {
    this.isa = NULL;
    this.version = 0;
    this.super_class = NULL;
    this.name = NULL;
    this.info = 0;

    this.ivar_list = [];
    this.ivar_store = function() {};
    this.ivar_dtable = this.ivar_store.prototype;

    this.method_list = [];
    this.method_store = function() {};
    this.method_dtable = this.method_store.prototype;

    this.protocol_list = [];
    this.allocator = function() {};

    this._UID = -1;
}

function objj_protocol(aName) {
    this.name = aName;
    this.instance_methods = {};
    this.class_methods = {};
}

function objj_object() {
    this.isa = NULL;
    this._UID = -1;
}

// =============================================================================
// Class Info Helpers
// =============================================================================

function _getClassInfo(aClass, mask) {
    return aClass.info & mask;
}

function _setClassInfo(aClass, mask) {
    aClass.info |= mask;
}

function _clearClassInfo(aClass, mask) {
    aClass.info &= ~mask;
}

function _isMetaClass(aClass) {
    return _getClassInfo(aClass, CLS_META);
}

function _getMetaClass(aClass) {
    return _isMetaClass(aClass) ? aClass : aClass.isa;
}

function _isInitialized(aClass) {
    return _getClassInfo(_getMetaClass(aClass), CLS_INITIALIZED);
}

// =============================================================================
// Working with Classes
// =============================================================================

function class_getName(aClass) {
    return aClass ? aClass.name : "";
}

function class_isMetaClass(aClass) {
    return aClass ? _isMetaClass(aClass) : NO;
}

function class_getSuperclass(aClass) {
    return aClass ? aClass.super_class : Nil;
}

function class_setSuperclass(aClass, aSuperClass) {
    aClass.super_class = aSuperClass;
    aClass.isa.super_class = aSuperClass.isa;
}

function class_addIvar(aClass, aName, aType) {
    var thePrototype = aClass.allocator.prototype;

    if (typeof thePrototype[aName] !== "undefined")
        return NO;

    var ivar = new objj_ivar(aName, aType);

    aClass.ivar_list.push(ivar);
    aClass.ivar_dtable[aName] = ivar;
    thePrototype[aName] = NULL;

    return YES;
}

function class_addIvars(aClass, ivars) {
    var thePrototype = aClass.allocator.prototype;

    for (var i = 0; i < ivars.length; i++) {
        var ivar = ivars[i],
            name = ivar.name;

        if (typeof thePrototype[name] === "undefined") {
            aClass.ivar_list.push(ivar);
            aClass.ivar_dtable[name] = ivar;
            thePrototype[name] = NULL;
        }
    }
}

function class_copyIvarList(aClass) {
    return aClass.ivar_list.slice(0);
}

function class_addMethod(aClass, aName, anImplementation, types) {
    var method = new objj_method(aName, anImplementation, types);

    aClass.method_list.push(method);
    aClass.method_dtable[aName] = method;

    // If this is a root class, add to metaclass too
    if (!_isMetaClass(aClass) && _getMetaClass(aClass).isa === _getMetaClass(aClass))
        class_addMethod(_getMetaClass(aClass), aName, anImplementation, types);

    return YES;
}

function class_addMethods(aClass, methods) {
    var method_list = aClass.method_list,
        method_dtable = aClass.method_dtable;

    for (var i = 0; i < methods.length; i++) {
        var method = methods[i];
        method_list.push(method);
        method_dtable[method.method_name] = method;
    }

    // If this is a root class, add to metaclass too
    if (!_isMetaClass(aClass) && _getMetaClass(aClass).isa === _getMetaClass(aClass))
        class_addMethods(_getMetaClass(aClass), methods);
}

function class_getInstanceMethod(aClass, aSelector) {
    if (!aClass || !aSelector)
        return NULL;

    return aClass.method_dtable[aSelector] || NULL;
}

function class_getClassMethod(aClass, aSelector) {
    if (!aClass || !aSelector)
        return NULL;

    return _getMetaClass(aClass).method_dtable[aSelector] || NULL;
}

function class_getInstanceVariable(aClass, aName) {
    if (!aClass || !aName)
        return NULL;

    return aClass.ivar_dtable[aName];
}

function class_respondsToSelector(aClass, aSelector) {
    return class_getClassMethod(aClass, aSelector) != NULL;
}

function class_copyMethodList(aClass) {
    return aClass.method_list.slice(0);
}

function class_replaceMethod(aClass, aSelector, aMethodImplementation) {
    if (!aClass || !aSelector)
        return NULL;

    var method = aClass.method_dtable[aSelector],
        oldImp = method.method_imp,
        newMethod = new objj_method(method.method_name, aMethodImplementation, method.method_types);

    aClass.method_dtable[aSelector] = newMethod;

    var index = aClass.method_list.indexOf(method);
    if (index !== -1)
        aClass.method_list[index] = newMethod;
    else
        aClass.method_list.push(newMethod);

    return oldImp;
}

function class_addProtocol(aClass, aProtocol) {
    if (!aProtocol || class_conformsToProtocol(aClass, aProtocol))
        return;

    (aClass.protocol_list || (aClass.protocol_list = [])).push(aProtocol);
    return true;
}

function class_conformsToProtocol(aClass, aProtocol) {
    if (!aProtocol)
        return false;

    while (aClass) {
        var protocols = aClass.protocol_list,
            size = protocols ? protocols.length : 0;

        for (var i = 0; i < size; i++) {
            var p = protocols[i];
            if (p.name === aProtocol.name || protocol_conformsToProtocol(p, aProtocol))
                return true;
        }

        aClass = class_getSuperclass(aClass);
    }

    return false;
}

function class_copyProtocolList(aClass) {
    return aClass.protocol_list ? aClass.protocol_list.slice(0) : [];
}

// =============================================================================
// Protocol Functions
// =============================================================================

function protocol_conformsToProtocol(p1, p2) {
    if (!p1 || !p2)
        return false;

    if (p1.name === p2.name)
        return true;

    var protocols = p1.protocol_list,
        size = protocols ? protocols.length : 0;

    for (var i = 0; i < size; i++) {
        var p = protocols[i];
        if (p.name === p2.name || protocol_conformsToProtocol(p, p2))
            return true;
    }

    return false;
}

function objj_allocateProtocol(aName) {
    return new objj_protocol(aName);
}

function objj_registerProtocol(proto) {
    REGISTERED_PROTOCOLS[proto.name] = proto;
}

function protocol_getName(proto) {
    return proto.name;
}

function protocol_addMethodDescription(proto, selector, types, isRequiredMethod, isInstanceMethod) {
    if (!proto || !selector) return;

    if (isRequiredMethod)
        (isInstanceMethod ? proto.instance_methods : proto.class_methods)[selector] = new objj_method(selector, null, types);
}

function objj_getProtocol(aName) {
    return REGISTERED_PROTOCOLS[aName];
}

// =============================================================================
// Class Allocation & Registration
// =============================================================================

function objj_allocateClassPair(superclass, aName) {
    var classObject = new objj_class(aName),
        metaClassObject = new objj_class(aName),
        rootClassObject = classObject;

    if (superclass) {
        rootClassObject = superclass;
        while (rootClassObject.super_class)
            rootClassObject = rootClassObject.super_class;

        // Inherit from superclass
        classObject.allocator.prototype = new superclass.allocator;
        classObject.ivar_dtable = classObject.ivar_store.prototype = new superclass.ivar_store;
        classObject.method_dtable = classObject.method_store.prototype = new superclass.method_store;
        metaClassObject.method_dtable = metaClassObject.method_store.prototype = new superclass.isa.method_store;

        classObject.super_class = superclass;
        metaClassObject.super_class = superclass.isa;
    } else {
        classObject.allocator.prototype = new objj_object();
    }

    classObject.isa = metaClassObject;
    classObject.name = aName;
    classObject.info = CLS_CLASS;
    classObject._UID = objj_generateObjectUID();

    metaClassObject.isa = rootClassObject.isa;
    metaClassObject.name = aName;
    metaClassObject.info = CLS_META;
    metaClassObject._UID = objj_generateObjectUID();

    return classObject;
}

function objj_registerClassPair(aClass) {
    REGISTERED_CLASSES[aClass.name] = aClass;

    // Make class globally accessible
    if (typeof global !== "undefined")
        global[aClass.name] = aClass;
    else if (typeof window !== "undefined")
        window[aClass.name] = aClass;
}

function objj_lookUpClass(aName) {
    return REGISTERED_CLASSES[aName] || Nil;
}

function objj_getClass(aName) {
    return REGISTERED_CLASSES[aName] || Nil;
}

function objj_getMetaClass(aName) {
    var theClass = objj_getClass(aName);
    return theClass ? _getMetaClass(theClass) : Nil;
}

function objj_getClassList(buffer, bufferLen) {
    for (var aName in REGISTERED_CLASSES) {
        buffer.push(REGISTERED_CLASSES[aName]);
        if (bufferLen && --bufferLen === 0)
            break;
    }
    return buffer.length;
}

// =============================================================================
// Instance Creation
// =============================================================================

function class_createInstance(aClass) {
    if (!aClass)
        throw new Error("*** Attempting to create object with Nil class.");

    var object = new aClass.allocator();
    object.isa = aClass;
    object._UID = objj_generateObjectUID();

    return object;
}

// =============================================================================
// Working with Instances
// =============================================================================

function object_getClassName(anObject) {
    if (!anObject)
        return "";

    var theClass = anObject.isa;
    return theClass ? class_getName(theClass) : "";
}

// =============================================================================
// Working with Instance Variables
// =============================================================================

function ivar_getName(anIvar) {
    return anIvar.name;
}

function ivar_getTypeEncoding(anIvar) {
    return anIvar.type;
}

// =============================================================================
// Working with Methods
// =============================================================================

function method_getName(aMethod) {
    return aMethod.method_name;
}

function method_getImplementation(aMethod) {
    return aMethod.method_imp;
}

function method_setImplementation(aMethod, anImplementation) {
    var oldImp = aMethod.method_imp;
    aMethod.method_imp = anImplementation;
    return oldImp;
}

function method_exchangeImplementations(lhs, rhs) {
    var lhsImp = method_getImplementation(lhs),
        rhsImp = method_getImplementation(rhs);

    method_setImplementation(lhs, rhsImp);
    method_setImplementation(rhs, lhsImp);
}

// =============================================================================
// Working with Selectors
// =============================================================================

function sel_getName(aSelector) {
    return aSelector ? aSelector : "<null selector>";
}

function sel_getUid(aName) {
    return aName;
}

function sel_isEqual(lhs, rhs) {
    return lhs === rhs;
}

function sel_registerName(aName) {
    return aName;
}

// =============================================================================
// Class Initialization
// =============================================================================

function _class_initialize(aClass) {
    var meta = _getMetaClass(aClass);

    if (_getClassInfo(aClass, CLS_META))
        aClass = objj_getClass(aClass.name);

    if (aClass.super_class && !_isInitialized(aClass.super_class))
        _class_initialize(aClass.super_class);

    if (!_getClassInfo(meta, CLS_INITIALIZED) && !_getClassInfo(meta, CLS_INITIALIZING)) {
        _setClassInfo(meta, CLS_INITIALIZING);

        // Call +initialize if it exists
        var initializeMethod = meta.method_dtable["initialize"];
        if (initializeMethod)
            initializeMethod.method_imp(aClass, "initialize");

        _clearClassInfo(meta, CLS_INITIALIZING);
        _setClassInfo(meta, CLS_INITIALIZED);
    }
}

// =============================================================================
// Message Forwarding
// =============================================================================

function _objj_forward(self, _cmd) {
    var isa = self.isa;

    // Try to initialize if needed
    if (!_isInitialized(isa))
        _class_initialize(isa);

    // Check again after initialization
    var method = isa.method_dtable[_cmd];
    if (method)
        return method.method_imp.apply(null, arguments);

    // Try forwardingTargetForSelector:
    var forwardingTarget = isa.method_dtable["forwardingTargetForSelector:"];
    if (forwardingTarget) {
        var target = forwardingTarget.method_imp(self, "forwardingTargetForSelector:", _cmd);
        if (target && target !== self) {
            arguments[0] = target;
            return objj_msgSend.apply(null, arguments);
        }
    }

    // Try doesNotRecognizeSelector:
    var doesNotRecognize = isa.method_dtable["doesNotRecognizeSelector:"];
    if (doesNotRecognize)
        return doesNotRecognize.method_imp(self, "doesNotRecognizeSelector:", _cmd);

    throw new Error(class_getName(isa) + " does not recognize selector '" + _cmd + "'");
}

// =============================================================================
// Message Sending
// =============================================================================

function objj_msgSend(aReceiver, aSelector) {
    if (aReceiver == nil)
        return nil;

    var isa = aReceiver.isa;

    // Initialize class if needed
    if (!_isInitialized(isa))
        _class_initialize(isa);

    var method = isa.method_dtable[aSelector];
    var implementation = method ? method.method_imp : _objj_forward;

    switch (arguments.length) {
        case 2: return implementation(aReceiver, aSelector);
        case 3: return implementation(aReceiver, aSelector, arguments[2]);
        case 4: return implementation(aReceiver, aSelector, arguments[2], arguments[3]);
        case 5: return implementation(aReceiver, aSelector, arguments[2], arguments[3], arguments[4]);
        case 6: return implementation(aReceiver, aSelector, arguments[2], arguments[3], arguments[4], arguments[5]);
    }

    return implementation.apply(null, arguments);
}

function objj_msgSendSuper(aSuper, aSelector) {
    var super_class = aSuper.super_class;
    var receiver = aSuper.receiver;

    if (!_isInitialized(super_class))
        _class_initialize(super_class);

    var method = super_class.method_dtable[aSelector];
    var implementation = method ? method.method_imp : _objj_forward;

    var args = Array.prototype.slice.call(arguments);
    args[0] = receiver;

    return implementation.apply(null, args);
}

// Optimized versions for specific argument counts
function objj_msgSend0(aReceiver, aSelector) {
    if (aReceiver == nil) return nil;
    var isa = aReceiver.isa;
    if (!_isInitialized(isa)) _class_initialize(isa);
    var method = isa.method_dtable[aSelector];
    return (method ? method.method_imp : _objj_forward)(aReceiver, aSelector);
}

function objj_msgSend1(aReceiver, aSelector, arg0) {
    if (aReceiver == nil) return nil;
    var isa = aReceiver.isa;
    if (!_isInitialized(isa)) _class_initialize(isa);
    var method = isa.method_dtable[aSelector];
    return (method ? method.method_imp : _objj_forward)(aReceiver, aSelector, arg0);
}

function objj_msgSend2(aReceiver, aSelector, arg0, arg1) {
    if (aReceiver == nil) return nil;
    var isa = aReceiver.isa;
    if (!_isInitialized(isa)) _class_initialize(isa);
    var method = isa.method_dtable[aSelector];
    return (method ? method.method_imp : _objj_forward)(aReceiver, aSelector, arg0, arg1);
}

function objj_msgSend3(aReceiver, aSelector, arg0, arg1, arg2) {
    if (aReceiver == nil) return nil;
    var isa = aReceiver.isa;
    if (!_isInitialized(isa)) _class_initialize(isa);
    var method = isa.method_dtable[aSelector];
    return (method ? method.method_imp : _objj_forward)(aReceiver, aSelector, arg0, arg1, arg2);
}

function objj_msgSendSuper0(aSuper, aSelector) {
    var method = aSuper.super_class.method_dtable[aSelector];
    return (method ? method.method_imp : _objj_forward)(aSuper.receiver, aSelector);
}

function objj_msgSendSuper1(aSuper, aSelector, arg0) {
    var method = aSuper.super_class.method_dtable[aSelector];
    return (method ? method.method_imp : _objj_forward)(aSuper.receiver, aSelector, arg0);
}

function objj_msgSendSuper2(aSuper, aSelector, arg0, arg1) {
    var method = aSuper.super_class.method_dtable[aSelector];
    return (method ? method.method_imp : _objj_forward)(aSuper.receiver, aSelector, arg0, arg1);
}

// =============================================================================
// Object String Representation
// =============================================================================

objj_class.prototype.toString = objj_object.prototype.toString = function() {
    var isa = this.isa;

    if (class_getInstanceMethod(isa, "description")) {
        var method = isa.method_dtable["description"];
        if (method)
            return method.method_imp(this, "description");
    }

    if (class_isMetaClass(isa))
        return this.name;

    return "[" + isa.name + " Object]";
};
